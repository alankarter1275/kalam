//! Author profile cache and alias lookup.

use super::*;

impl Catalog {
    pub fn get_author_profile_by_name(&self, name: &str) -> Result<Option<AuthorProfile>> {
        let key = crate::author::normalize_author_name(name);
        if key.is_empty() {
            return Ok(None);
        }
        let conn = self.conn();
        let Some(id) = profile_id_by_key(&conn, &key)? else {
            return Ok(None);
        };
        load_author_profile(&conn, id)
    }

    /// Every cached author profile, in sort-name order. The Authors
    /// index joins these with the books in the service layer; one
    /// profile query plus one alias query, however many authors there
    /// are.
    pub fn all_author_profiles(&self) -> Result<Vec<AuthorProfile>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {PROFILE_COLUMNS} FROM author_profiles
             ORDER BY sort_name COLLATE NOCASE ASC, canonical_name COLLATE NOCASE ASC"
        ))?;
        let mut profiles = stmt
            .query_map([], profile_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut stmt = conn.prepare_cached(
            "SELECT author_id, alias FROM author_aliases
             ORDER BY alias COLLATE NOCASE ASC",
        )?;
        let alias_rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut aliases: HashMap<i64, Vec<String>> = HashMap::new();
        for (author_id, alias) in alias_rows {
            aliases.entry(author_id).or_default().push(alias);
        }
        for profile in &mut profiles {
            profile.aliases = aliases.remove(&profile.id).unwrap_or_default();
        }
        Ok(profiles)
    }

    /// Store a fetched profile. Fields the reader edited by hand
    /// (`manual_fields` on the stored row) survive the refresh
    /// (decision 48): the stored value replaces the fetched one
    /// before anything is written.
    pub fn upsert_author_profile(&self, profile: &AuthorProfile) -> Result<i64> {
        let conn = self.conn();
        let mut effective = profile.clone();
        let key = if effective.normalized_name.trim().is_empty() {
            crate::author::normalize_author_name(&effective.canonical_name)
        } else {
            effective.normalized_name.trim().to_string()
        };
        let existing = profile_id_by_key(&conn, &key)?
            .map(|id| load_author_profile(&conn, id))
            .transpose()?
            .flatten()
            .or({
                let canonical_key =
                    crate::author::normalize_author_name(&effective.canonical_name);
                if canonical_key == key {
                    None
                } else {
                    profile_id_by_key(&conn, &canonical_key)?
                        .map(|id| load_author_profile(&conn, id))
                        .transpose()?
                        .flatten()
                }
            });

        let existing_id = match &existing {
            Some(existing) => {
                apply_manual_fields(&mut effective, existing);
                // The old upsert kept a downloaded portrait when a
                // refresh produced none; keep that behaviour outside
                // the lock logic too.
                if effective.photo_file.is_none() {
                    effective.photo_file = existing.photo_file.clone();
                    effective.photo_path = existing.photo_path.clone();
                }
                effective.id = existing.id;
                Some(existing.id)
            }
            None => None,
        };

        write_author_profile(&conn, &effective, existing_id, &chrono_like_now())
    }

    /// Edit one author (decision 112): rename the identity on every
    /// book and comic series that carries it, rewrite the cached
    /// profile, and lock each changed field against later metadata
    /// fetches (decision 48). One transaction: the books, the
    /// profile, the author shelf link, smart-shelf rules and the
    /// series cache move together or not at all. Book folders and
    /// sidecars follow after the commit, best-effort like every
    /// other metadata edit.
    pub fn edit_author(
        &self,
        current_name: &str,
        edit: &AuthorEdit,
    ) -> Result<AuthorEditOutcome> {
        let name = crate::author::display_author_name(edit.name.trim());
        if name.is_empty() {
            return Err(DbError::Author("The author needs a name.".into()));
        }
        let new_key = crate::author::normalize_author_name(&name);
        let sort_name = if edit.sort_name.trim().is_empty() {
            crate::author::sort_author_name(&name)
        } else {
            edit.sort_name.trim().to_string()
        };

        let mut conn = self.conn();
        let existing = profile_by_name_conn(&conn, current_name)?;
        let source_keys = identity_keys(current_name, existing.as_ref());
        let rows = author_book_rows(&conn)?;
        let comic_rows = comic_series_author_rows(&conn)?;
        let rewrites = plan_author_rewrites(&rows, &source_keys, &name);
        let comic_rewrites = plan_comic_rewrites(&comic_rows, &source_keys, &name);
        if existing.is_none() && rewrites.is_empty() && comic_rewrites.is_empty() {
            return Err(DbError::Author(
                "That author is not in your library anymore.".into(),
            ));
        }

        // Renaming onto a different author who is already here is a
        // merge, not an edit — refuse it so the two identities can
        // never silently fuse through the wrong door.
        let already_exists = format!(
            "“{name}” is already another author in your library. \
             Use Merge into another author… to combine them."
        );
        if let Some(target) = profile_by_name_conn(&conn, &name)? {
            if existing.as_ref().map(|p| p.id) != Some(target.id) {
                return Err(DbError::Author(already_exists));
            }
        }
        if !source_keys.contains(&new_key)
            && (rows.iter().any(|row| authors_contain_key(&row.authors, &new_key))
                || comic_rows
                    .iter()
                    .any(|row| authors_contain_key(&row.1, &new_key)))
        {
            return Err(DbError::Author(already_exists));
        }

        // An alias that already names different books is a second
        // author wearing this one's profile; that, too, is a merge.
        for alias in &edit.aliases {
            let alias_key = crate::author::normalize_author_name(alias);
            if alias_key.is_empty() || source_keys.contains(&alias_key) || alias_key == new_key {
                continue;
            }
            let taken = profile_by_name_conn(&conn, alias)?
                .is_some_and(|p| existing.as_ref().map(|e| e.id) != Some(p.id))
                || rows.iter().any(|row| authors_contain_key(&row.authors, &alias_key))
                || comic_rows
                    .iter()
                    .any(|row| authors_contain_key(&row.1, &alias_key));
            if taken {
                return Err(DbError::Author(format!(
                    "“{}” already belongs to another author. \
                     Use Merge into another author… to combine them.",
                    alias.trim()
                )));
            }
        }

        let effective_name = existing
            .as_ref()
            .map(|p| p.canonical_name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| crate::author::display_author_name(current_name));
        let effective_sort = existing
            .as_ref()
            .map(|p| p.sort_name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| crate::author::sort_author_name(&effective_name));

        let mut profile = existing.clone().unwrap_or_default();
        let mut manual_fields = profile.manual_fields.clone();
        let mut lock = |field: &str, changed: bool| {
            if changed && !manual_fields.iter().any(|f| f == field) {
                manual_fields.push(field.to_string());
            }
        };
        lock(AUTHOR_FIELD_NAME, name != effective_name);
        lock(AUTHOR_FIELD_SORT_NAME, sort_name != effective_sort);
        lock(AUTHOR_FIELD_BIO, edit.bio.trim() != profile.bio.trim());
        lock(
            AUTHOR_FIELD_BIRTH_DATE,
            edit.birth_date.trim() != profile.birth_date.trim(),
        );
        lock(
            AUTHOR_FIELD_DEATH_DATE,
            edit.death_date.trim() != profile.death_date.trim(),
        );

        let mut final_aliases: Vec<String> = edit.aliases.clone();
        final_aliases.push(name.clone());
        final_aliases.push(sort_name.clone());
        final_aliases.push(current_name.to_string());
        final_aliases.push(effective_name.clone());
        final_aliases.push(effective_sort.clone());
        let final_aliases = dedup_aliases(final_aliases);
        let old_alias_keys: std::collections::HashSet<String> = profile
            .aliases
            .iter()
            .map(|a| crate::author::normalize_author_name(a))
            .collect();
        let new_alias_keys: std::collections::HashSet<String> = final_aliases
            .iter()
            .map(|a| crate::author::normalize_author_name(a))
            .collect();
        lock(
            AUTHOR_FIELD_ALIASES,
            existing.is_some() && old_alias_keys != new_alias_keys,
        );

        match &edit.photo {
            AuthorPhotoEdit::Keep => {}
            AuthorPhotoEdit::Remove => {
                lock(AUTHOR_FIELD_PHOTO, profile.photo_file.is_some());
                profile.photo_file = None;
                profile.photo_path = None;
            }
            AuthorPhotoEdit::Replace(file) => {
                lock(
                    AUTHOR_FIELD_PHOTO,
                    profile.photo_file.as_deref() != Some(file.as_str()),
                );
                profile.photo_file = Some(file.clone());
                profile.photo_path = Some(crate::paths::authors_dir().join(file));
            }
        }

        profile.canonical_name = name.clone();
        profile.sort_name = sort_name;
        profile.normalized_name = new_key;
        profile.bio = edit.bio.trim().to_string();
        profile.birth_date = edit.birth_date.trim().to_string();
        profile.death_date = edit.death_date.trim().to_string();
        profile.aliases = final_aliases;
        profile.manual_fields = manual_fields;
        let fetched_at = profile.fetched_at.clone();
        let existing_id = existing.as_ref().map(|p| p.id);
        let books_updated = rewrites.len();

        let tx = conn.transaction()?;
        apply_author_rewrites(&tx, &rewrites, &comic_rewrites)?;
        write_author_profile(&tx, &profile, existing_id, &fetched_at)?;
        migrate_author_shelf_links(&tx, &source_keys, &name)?;
        migrate_series_cache(&tx, &rewrites, &source_keys, &name)?;
        tx.commit()?;
        drop(conn);

        self.refresh_renamed_author_books(&rewrites);
        Ok(AuthorEditOutcome {
            name,
            books_updated,
        })
    }

    /// Merge one author into another (decision 112): every book and
    /// comic series under the source name moves to the target, the
    /// source name and its spellings stay behind as aliases of the
    /// target, and the two cached profiles fold into one — target
    /// values win, source values fill the gaps, works are unioned,
    /// and hand-edited locks travel with the values they protect.
    pub fn merge_authors(
        &self,
        source_name: &str,
        target_name: &str,
    ) -> Result<AuthorEditOutcome> {
        let source_display = crate::author::display_author_name(source_name.trim());
        let target_display = crate::author::display_author_name(target_name.trim());
        if source_display.is_empty() || target_display.is_empty() {
            return Err(DbError::Author(
                "Choose an author to merge into.".into(),
            ));
        }

        let mut conn = self.conn();
        let source_profile = profile_by_name_conn(&conn, &source_display)?;
        let target_profile = profile_by_name_conn(&conn, &target_display)?;
        let source_keys = identity_keys(&source_display, source_profile.as_ref());
        let target_keys = identity_keys(&target_display, target_profile.as_ref());
        let same_profile = matches!(
            (source_profile.as_ref(), target_profile.as_ref()),
            (Some(s), Some(t)) if s.id == t.id
        );
        if same_profile || !source_keys.is_disjoint(&target_keys) {
            return Err(DbError::Author(
                "Choose a different author to merge into.".into(),
            ));
        }

        let canonical = target_profile
            .as_ref()
            .map(|p| p.canonical_name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| target_display.clone());
        let target_key = crate::author::normalize_author_name(&canonical);

        let rows = author_book_rows(&conn)?;
        let comic_rows = comic_series_author_rows(&conn)?;
        let rewrites = plan_author_rewrites(&rows, &source_keys, &canonical);
        let comic_rewrites = plan_comic_rewrites(&comic_rows, &source_keys, &canonical);
        if source_profile.is_none() && rewrites.is_empty() && comic_rewrites.is_empty() {
            return Err(DbError::Author(
                "That author is not in your library anymore.".into(),
            ));
        }
        let target_present = target_profile.is_some()
            || rows
                .iter()
                .any(|row| authors_match_any(&row.authors, &target_keys))
            || comic_rows
                .iter()
                .any(|row| authors_match_any(&row.1, &target_keys));
        if !target_present {
            return Err(DbError::Author(format!(
                "“{canonical}” is not in your library anymore."
            )));
        }

        let profile = merged_author_profile(
            source_profile.as_ref(),
            target_profile.as_ref(),
            &source_display,
            &canonical,
            &target_key,
        );
        let fetched_at = profile.fetched_at.clone();
        let existing_id = target_profile
            .as_ref()
            .or(source_profile.as_ref())
            .map(|p| p.id);
        let delete_source_id = match (source_profile.as_ref(), target_profile.as_ref()) {
            (Some(source), Some(_)) => Some(source.id),
            _ => None,
        };
        let books_updated = rewrites.len();

        let tx = conn.transaction()?;
        apply_author_rewrites(&tx, &rewrites, &comic_rewrites)?;
        write_author_profile(&tx, &profile, existing_id, &fetched_at)?;
        if let Some(source_id) = delete_source_id {
            tx.execute(
                "DELETE FROM author_profiles WHERE id = ?1",
                params![source_id],
            )?;
        }
        migrate_author_shelf_links(&tx, &source_keys, &canonical)?;
        migrate_series_cache(&tx, &rewrites, &source_keys, &canonical)?;
        tx.commit()?;
        drop(conn);

        self.refresh_renamed_author_books(&rewrites);
        Ok(AuthorEditOutcome {
            name: canonical,
            books_updated,
        })
    }

    /// After an author rewrite commits: remember each book's new
    /// metadata, move its folder, refresh its sidecar — the same
    /// best-effort tail `update_book_metadata` runs.
    fn refresh_renamed_author_books(&self, rewrites: &[AuthorRewrite]) {
        for rewrite in rewrites {
            let _ = self.remember_overrides(rewrite.book_id);
            crate::folders::rename_book_folder_if_needed(
                &rewrite.uuid,
                &rewrite.new_authors,
                &rewrite.title,
            );
            crate::sidecar::refresh_for_book(self, rewrite.book_id);
        }
    }

    pub fn books_for_author(&self, author_name: &str) -> Result<Vec<Book>> {
        let display_name = crate::author::display_author_name(author_name);
        if crate::author::normalize_author_name(&display_name).is_empty() {
            return Ok(Vec::new());
        }

        // The identity is bigger than the spelling asked for: a
        // cached profile's aliases are the same person (pen names,
        // alternate spellings), and the Authors index folds them
        // together — this lookup must agree or the two pages would
        // count different libraries. Still one candidate query: the
        // variants only widen its LIKE patterns.
        let profile = self.get_author_profile_by_name(author_name)?;
        let target_keys = identity_keys(author_name, profile.as_ref());

        let mut variants: Vec<String> = Vec::new();
        let mut add_variant = |value: &str| {
            for form in [
                crate::author::display_author_name(value),
                crate::author::sort_author_name(value),
                value.trim().to_string(),
            ] {
                if !form.is_empty()
                    && !variants
                        .iter()
                        .any(|v: &String| v.eq_ignore_ascii_case(&form))
                {
                    variants.push(form);
                }
            }
        };
        add_variant(&display_name);
        add_variant(author_name);
        if let Some(profile) = &profile {
            add_variant(&profile.canonical_name);
            add_variant(&profile.sort_name);
            for alias in &profile.aliases {
                add_variant(alias);
            }
        }
        // Bound the generated SQL for an author with an unusual
        // number of recorded spellings; the canonical forms are
        // first, so the cap only sheds the obscure tail.
        variants.truncate(40);

        let mut clauses: Vec<String> = Vec::new();
        let mut patterns: Vec<String> = Vec::new();
        for variant in &variants {
            for pattern in [
                format!("{}%", escape_like(variant)),
                format!("% {}%", escape_like(variant)),
            ] {
                patterns.push(pattern);
                let index = patterns.len();
                clauses.push(format!(
                    "SELECT id FROM books WHERE books.authors LIKE ?{index} ESCAPE '\\'"
                ));
                patterns.push(patterns[index - 1].clone());
                let index = patterns.len();
                clauses.push(format!(
                    "SELECT c.book_id FROM comic_chapters c \
                     JOIN comic_series s ON s.id = c.series_id \
                     WHERE s.author LIKE ?{index} ESCAPE '\\'"
                ));
            }
        }

        let conn = self.conn();
        let sql = format!(
            "SELECT {BOOK_COLUMNS} FROM books
             WHERE books.id IN ({})
             ORDER BY sort_title COLLATE NOCASE ASC",
            clauses.join(" UNION ")
        );
        let candidate_rows = {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(patterns.iter()),
                row_to_book,
            )?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        // A chapter's own author string can be blank while its
        // series names the author; those candidates are judged by
        // the series credit, the way the collapsed lists show them.
        let comic_ids: Vec<i64> = candidate_rows
            .iter()
            .filter(|b| matches!(b.format, BookFormat::Cbz | BookFormat::Cbr))
            .map(|b| b.id)
            .collect();
        let mut comic_author: HashMap<i64, String> = HashMap::new();
        if !comic_ids.is_empty() {
            let holders = vec!["?"; comic_ids.len()].join(",");
            let sql = format!(
                "SELECT c.book_id, s.author FROM comic_chapters c
                 JOIN comic_series s ON s.id = c.series_id
                 WHERE c.book_id IN ({holders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(comic_ids.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (book_id, author) = row?;
                comic_author.insert(book_id, author);
            }
        }

        let mut books: Vec<Book> = candidate_rows
            .into_iter()
            .filter(|book| {
                authors_match_any(book.authors_display(), &target_keys)
                    || comic_author
                        .get(&book.id)
                        .is_some_and(|author| authors_match_any(author, &target_keys))
            })
            .collect();

        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }
}

/// The profile columns, in the order [`profile_from_row`] reads them.
/// One list for every SELECT so the indices cannot drift apart.
const PROFILE_COLUMNS: &str = "id, canonical_name, sort_name, normalized_name, bio, \
    birth_date, death_date, top_work, top_subjects_json, openlibrary_key, \
    photo_file, work_count, works_json, fetched_at, source_url, manual_fields_json";

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthorProfile> {
    let photo_file: Option<String> = row.get(10)?;
    let top_subjects = json_vec_string(&row.get::<_, String>(8)?);
    let works = json_author_works(&row.get::<_, String>(12)?);
    let manual_fields = json_vec_string(&row.get::<_, String>(15)?);
    Ok(AuthorProfile {
        id: row.get(0)?,
        canonical_name: row.get(1)?,
        sort_name: row.get(2)?,
        normalized_name: row.get(3)?,
        bio: row.get(4)?,
        birth_date: row.get(5)?,
        death_date: row.get(6)?,
        top_work: row.get(7)?,
        top_subjects,
        openlibrary_key: row.get(9)?,
        photo_path: photo_file
            .as_ref()
            .map(|file| crate::paths::authors_dir().join(file)),
        photo_file,
        work_count: row.get(11)?,
        works,
        aliases: Vec::new(),
        fetched_at: row.get(13)?,
        source_url: row.get(14)?,
        manual_fields,
    })
}

fn profile_id_by_key(conn: &Connection, key: &str) -> Result<Option<i64>> {
    if key.is_empty() {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT id FROM author_profiles WHERE normalized_name = ?1
             UNION
             SELECT author_id FROM author_aliases WHERE normalized_alias = ?1
             LIMIT 1",
            params![key],
            |r| r.get::<_, i64>(0),
        )
        .optional()?)
}

fn profile_by_name_conn(conn: &Connection, name: &str) -> Result<Option<AuthorProfile>> {
    let key = crate::author::normalize_author_name(name);
    let Some(id) = profile_id_by_key(conn, &key)? else {
        return Ok(None);
    };
    load_author_profile(conn, id)
}

fn load_author_profile(conn: &Connection, id: i64) -> Result<Option<AuthorProfile>> {
    let row = conn
        .query_row(
            &format!("SELECT {PROFILE_COLUMNS} FROM author_profiles WHERE id = ?1"),
            params![id],
            profile_from_row,
        )
        .optional()?;
    let Some(mut profile) = row else {
        return Ok(None);
    };

    let mut stmt = conn.prepare_cached(
        "SELECT alias FROM author_aliases WHERE author_id = ?1 ORDER BY alias COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map(params![id], |r| r.get::<_, String>(0))?;
    profile.aliases = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Some(profile))
}

/// Re-apply the stored hand edits over a freshly fetched profile.
/// A field the incoming value itself claims as hand-edited (an Edit
/// author save routed through here) keeps the incoming value; every
/// other locked field takes the stored one.
fn apply_manual_fields(incoming: &mut AuthorProfile, existing: &AuthorProfile) {
    let incoming_locks: std::collections::HashSet<&str> =
        incoming.manual_fields.iter().map(String::as_str).collect();
    for field in &existing.manual_fields {
        if incoming_locks.contains(field.as_str()) {
            continue;
        }
        match field.as_str() {
            AUTHOR_FIELD_NAME => {
                incoming.canonical_name = existing.canonical_name.clone();
                incoming.normalized_name = existing.normalized_name.clone();
            }
            AUTHOR_FIELD_SORT_NAME => incoming.sort_name = existing.sort_name.clone(),
            AUTHOR_FIELD_BIO => incoming.bio = existing.bio.clone(),
            AUTHOR_FIELD_BIRTH_DATE => incoming.birth_date = existing.birth_date.clone(),
            AUTHOR_FIELD_DEATH_DATE => incoming.death_date = existing.death_date.clone(),
            AUTHOR_FIELD_ALIASES => incoming.aliases = existing.aliases.clone(),
            AUTHOR_FIELD_PHOTO => {
                incoming.photo_file = existing.photo_file.clone();
                incoming.photo_path = existing.photo_path.clone();
            }
            _ => {}
        }
    }
    let mut locks = existing.manual_fields.clone();
    for field in &incoming.manual_fields {
        if !locks.contains(field) {
            locks.push(field.clone());
        }
    }
    incoming.manual_fields = locks;
}

/// Write one profile row and its aliases. With `existing_id` the row
/// is updated in place (a rename keeps its works and fetched state);
/// without one it is inserted, or updated on a normalized-name
/// conflict. `fetched_at` is the caller's choice: a fetch stamps
/// now, an edit preserves the profile's own stamp.
fn write_author_profile(
    conn: &Connection,
    profile: &AuthorProfile,
    existing_id: Option<i64>,
    fetched_at: &str,
) -> Result<i64> {
    let canonical_name = profile.canonical_name.trim();
    let sort_name = if profile.sort_name.trim().is_empty() {
        crate::author::sort_author_name(canonical_name)
    } else {
        profile.sort_name.trim().to_string()
    };
    let normalized_name = if profile.normalized_name.trim().is_empty() {
        crate::author::normalize_author_name(canonical_name)
    } else {
        profile.normalized_name.trim().to_string()
    };
    let subjects_json =
        serde_json::to_string(&profile.top_subjects).unwrap_or_else(|_| "[]".into());
    let works_json = serde_json::to_string(&profile.works).unwrap_or_else(|_| "[]".into());
    let manual_json =
        serde_json::to_string(&profile.manual_fields).unwrap_or_else(|_| "[]".into());

    let author_id = match existing_id {
        Some(id) => {
            conn.execute(
                "UPDATE author_profiles SET
                    canonical_name = ?2, sort_name = ?3, normalized_name = ?4,
                    bio = ?5, birth_date = ?6, death_date = ?7, top_work = ?8,
                    top_subjects_json = ?9, openlibrary_key = ?10, photo_file = ?11,
                    work_count = ?12, works_json = ?13, fetched_at = ?14,
                    source_url = ?15, manual_fields_json = ?16
                 WHERE id = ?1",
                params![
                    id,
                    canonical_name,
                    sort_name,
                    normalized_name,
                    profile.bio.trim(),
                    profile.birth_date.trim(),
                    profile.death_date.trim(),
                    profile.top_work.trim(),
                    subjects_json,
                    profile.openlibrary_key.trim(),
                    profile.photo_file.as_deref(),
                    profile.work_count,
                    works_json,
                    fetched_at,
                    profile.source_url.trim(),
                    manual_json,
                ],
            )?;
            conn.execute(
                "DELETE FROM author_aliases WHERE author_id = ?1",
                params![id],
            )?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO author_profiles
                    (canonical_name, sort_name, normalized_name, bio, birth_date, death_date,
                     top_work, top_subjects_json, openlibrary_key, photo_file, work_count,
                     works_json, fetched_at, source_url, manual_fields_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(normalized_name) DO UPDATE SET
                    canonical_name = excluded.canonical_name,
                    sort_name = excluded.sort_name,
                    bio = excluded.bio,
                    birth_date = excluded.birth_date,
                    death_date = excluded.death_date,
                    top_work = excluded.top_work,
                    top_subjects_json = excluded.top_subjects_json,
                    openlibrary_key = excluded.openlibrary_key,
                    photo_file = excluded.photo_file,
                    work_count = excluded.work_count,
                    works_json = excluded.works_json,
                    fetched_at = excluded.fetched_at,
                    source_url = excluded.source_url,
                    manual_fields_json = excluded.manual_fields_json",
                params![
                    canonical_name,
                    sort_name,
                    normalized_name,
                    profile.bio.trim(),
                    profile.birth_date.trim(),
                    profile.death_date.trim(),
                    profile.top_work.trim(),
                    subjects_json,
                    profile.openlibrary_key.trim(),
                    profile.photo_file.as_deref(),
                    profile.work_count,
                    works_json,
                    fetched_at,
                    profile.source_url.trim(),
                    manual_json,
                ],
            )?;
            conn.query_row(
                "SELECT id FROM author_profiles WHERE normalized_name = ?1",
                params![normalized_name],
                |r| r.get(0),
            )?
        }
    };

    let mut aliases = profile.aliases.clone();
    aliases.push(canonical_name.to_string());
    aliases.push(sort_name);
    let mut seen = std::collections::HashSet::new();
    for alias in aliases {
        let alias = alias.trim();
        if alias.is_empty() {
            continue;
        }
        let normalized = crate::author::normalize_author_name(alias);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        conn.execute(
            "INSERT INTO author_aliases (author_id, alias, normalized_alias)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(normalized_alias) DO UPDATE SET
                 author_id = excluded.author_id,
                 alias = excluded.alias",
            params![author_id, alias, normalized],
        )?;
    }
    Ok(author_id)
}

/// One book row as the identity rewrites read it.
struct AuthorBookRow {
    id: i64,
    uuid: String,
    title: String,
    authors: String,
    series: Option<String>,
}

/// One book whose author string an edit/merge will rewrite.
struct AuthorRewrite {
    book_id: i64,
    uuid: String,
    title: String,
    old_authors: String,
    new_authors: String,
    series: Option<String>,
}

/// Every normalized spelling that means this author: the name asked
/// for plus the cached profile's canonical/sort/alias forms.
fn identity_keys(
    name: &str,
    profile: Option<&AuthorProfile>,
) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    let mut add = |value: &str| {
        let key = crate::author::normalize_author_name(value);
        if !key.is_empty() {
            keys.insert(key);
        }
    };
    add(name);
    if let Some(profile) = profile {
        add(&profile.canonical_name);
        add(&profile.sort_name);
        if !profile.normalized_name.trim().is_empty() {
            // Normalizing an already-normalized key returns it.
            add(profile.normalized_name.trim());
        }
        for alias in &profile.aliases {
            add(alias);
        }
    }
    keys
}

fn dedup_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for alias in aliases {
        let alias = alias.trim().to_string();
        if alias.is_empty() {
            continue;
        }
        let key = crate::author::normalize_author_name(&alias);
        if !key.is_empty() && seen.insert(key) {
            out.push(alias);
        }
    }
    out
}

fn authors_contain_key(authors: &str, key: &str) -> bool {
    crate::author::split_author_names(authors)
        .iter()
        .any(|name| crate::author::normalize_author_name(name) == key)
        || crate::author::normalize_author_name(authors) == key
}

fn authors_match_any(authors: &str, keys: &std::collections::HashSet<String>) -> bool {
    crate::author::split_author_names(authors)
        .iter()
        .any(|name| keys.contains(&crate::author::normalize_author_name(name)))
        || keys.contains(&crate::author::normalize_author_name(authors))
}

/// Replace every source-identity name in an author string with the
/// new canonical name, dropping the duplicate when both names were
/// credited on the same book. `None` when the source is not in the
/// string at all — untouched books keep their exact spelling.
fn rewrite_author_string(
    authors: &str,
    source_keys: &std::collections::HashSet<String>,
    new_name: &str,
) -> Option<String> {
    let names = crate::author::split_author_names(authors);
    if names.is_empty()
        || !names
            .iter()
            .any(|name| source_keys.contains(&crate::author::normalize_author_name(name)))
    {
        // A whole-string sort form ("Martin, George R. R.") splits
        // to itself, so this also covers the single-author case.
        if !source_keys.contains(&crate::author::normalize_author_name(authors)) {
            return None;
        }
        return Some(new_name.to_string());
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in names {
        let replacement =
            if source_keys.contains(&crate::author::normalize_author_name(&name)) {
                new_name.to_string()
            } else {
                name
            };
        if seen.insert(crate::author::normalize_author_name(&replacement)) {
            out.push(replacement);
        }
    }
    Some(out.join("; "))
}

fn author_book_rows(conn: &Connection) -> Result<Vec<AuthorBookRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, uuid, title, authors, series FROM books ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(AuthorBookRow {
            id: r.get(0)?,
            uuid: r.get(1)?,
            title: r.get(2)?,
            authors: r.get(3)?,
            series: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn comic_series_author_rows(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt =
        conn.prepare_cached("SELECT id, author FROM comic_series ORDER BY id ASC")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn plan_author_rewrites(
    rows: &[AuthorBookRow],
    source_keys: &std::collections::HashSet<String>,
    new_name: &str,
) -> Vec<AuthorRewrite> {
    rows.iter()
        .filter_map(|row| {
            let new_authors = rewrite_author_string(&row.authors, source_keys, new_name)?;
            (new_authors != row.authors).then(|| AuthorRewrite {
                book_id: row.id,
                uuid: row.uuid.clone(),
                title: row.title.clone(),
                old_authors: row.authors.clone(),
                new_authors,
                series: row.series.clone(),
            })
        })
        .collect()
}

fn plan_comic_rewrites(
    rows: &[(i64, String)],
    source_keys: &std::collections::HashSet<String>,
    new_name: &str,
) -> Vec<(i64, String)> {
    rows.iter()
        .filter_map(|(id, author)| {
            let new_author = rewrite_author_string(author, source_keys, new_name)?;
            (new_author != *author).then(|| (*id, new_author))
        })
        .collect()
}

fn apply_author_rewrites(
    conn: &Connection,
    rewrites: &[AuthorRewrite],
    comic_rewrites: &[(i64, String)],
) -> Result<()> {
    for rewrite in rewrites {
        conn.execute(
            "UPDATE books SET authors = ?2 WHERE id = ?1",
            params![rewrite.book_id, rewrite.new_authors],
        )?;
    }
    for (series_id, author) in comic_rewrites {
        conn.execute(
            "UPDATE comic_series SET author = ?2 WHERE id = ?1",
            params![series_id, author],
        )?;
    }
    Ok(())
}

/// The first-author form the series cache keys on. The service
/// derives it with a comma split, so the migration must too, or it
/// would rename keys nobody looks up.
fn first_series_author(authors: &str) -> &str {
    authors.split(',').next().unwrap_or("").trim()
}

/// Move cached series listings from the old first-author key to the
/// new one. When both keys have a listing the target's wins and the
/// source's is dropped.
fn migrate_series_cache(
    conn: &Connection,
    rewrites: &[AuthorRewrite],
    source_keys: &std::collections::HashSet<String>,
    new_name: &str,
) -> Result<()> {
    let mut remaps: Vec<(String, String)> = Vec::new();
    for rewrite in rewrites {
        let Some(series) = rewrite.series.as_deref().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let old_key = series_key(series, first_series_author(&rewrite.old_authors));
        let new_key = series_key(series, first_series_author(&rewrite.new_authors));
        if old_key != new_key && !remaps.contains(&(old_key.clone(), new_key.clone())) {
            remaps.push((old_key, new_key));
        }
    }

    for (old_key, new_key) in remaps {
        let works_json: Option<String> = conn
            .query_row(
                "SELECT works_json FROM series_cache WHERE series_key = ?1",
                params![old_key],
                |r| r.get(0),
            )
            .optional()?;
        let Some(works_json) = works_json else { continue };
        let target_exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM series_cache WHERE series_key = ?1",
            params![new_key],
            |r| r.get(0),
        )?;
        if target_exists > 0 {
            conn.execute(
                "DELETE FROM series_cache WHERE series_key = ?1",
                params![old_key],
            )?;
            continue;
        }
        let works_json = match serde_json::from_str::<Vec<SeriesWork>>(&works_json) {
            Ok(mut works) => {
                for work in &mut works {
                    if authors_match_any(&work.author, source_keys) {
                        work.author = new_name.to_string();
                    }
                }
                serde_json::to_string(&works).unwrap_or(works_json)
            }
            Err(_) => works_json,
        };
        conn.execute(
            "UPDATE series_cache SET series_key = ?2, works_json = ?3 WHERE series_key = ?1",
            params![old_key, new_key, works_json],
        )?;
    }
    Ok(())
}

/// Carry the author-page shelf link and every smart shelf's author
/// rules from the old identity to the new name. A linked shelf —
/// one the author page created to follow the author — is renamed
/// too, unless another shelf already owns the new name.
fn migrate_author_shelf_links(
    conn: &Connection,
    source_keys: &std::collections::HashSet<String>,
    new_name: &str,
) -> Result<()> {
    use crate::shelf_rules::{RuleField, RuleSet};

    // The key shape is `author.shelf.<normalized name>`, owned by
    // the service constant of the same spelling.
    let target_pref = format!(
        "author.shelf.{}",
        crate::author::normalize_author_name(new_name)
    );
    let prefs: Vec<(String, String)> = {
        let mut stmt = conn.prepare_cached(
            "SELECT key, value FROM app_prefs WHERE key LIKE 'author.shelf.%'",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let source_pref = prefs.iter().find(|(key, _)| {
        key != &target_pref
            && key
                .strip_prefix("author.shelf.")
                .is_some_and(|suffix| source_keys.contains(suffix))
    });
    let has_target_pref = prefs.iter().any(|(key, _)| key == &target_pref);
    if let (Some((_, value)), false) = (source_pref, has_target_pref) {
        conn.execute(
            "INSERT INTO app_prefs (key, value) VALUES (?1, ?2)",
            params![target_pref, value],
        )?;
        if value.ends_with(":linked") {
            if let Some(shelf_id) = value
                .split(':')
                .next()
                .and_then(|id| id.parse::<i64>().ok())
            {
                let collision: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM shelves
                     WHERE name = ?1 COLLATE NOCASE AND id <> ?2",
                    params![new_name, shelf_id],
                    |r| r.get(0),
                )?;
                if collision == 0 {
                    conn.execute(
                        "UPDATE shelves SET name = ?2, updated_at = ?3 WHERE id = ?1",
                        params![shelf_id, new_name, chrono_like_now()],
                    )?;
                }
            }
        }
    }
    for (key, _) in &prefs {
        if key != &target_pref
            && key
                .strip_prefix("author.shelf.")
                .is_some_and(|suffix| source_keys.contains(suffix))
        {
            conn.execute("DELETE FROM app_prefs WHERE key = ?1", params![key])?;
        }
    }

    let shelves: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare_cached("SELECT id, rules FROM shelves WHERE kind = 'smart'")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (shelf_id, rules_json) in shelves {
        if rules_json.trim().is_empty() {
            continue;
        }
        let mut ruleset = RuleSet::parse(&rules_json);
        let mut changed = false;
        for rule in &mut ruleset.rules {
            if rule.field_enum() == Some(RuleField::Author)
                && source_keys.contains(&crate::author::normalize_author_name(&rule.value))
                && rule.value != new_name
            {
                rule.value = new_name.to_string();
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE shelves SET rules = ?2, updated_at = ?3 WHERE id = ?1",
                params![shelf_id, ruleset.to_json(), chrono_like_now()],
            )?;
        }
    }
    Ok(())
}

fn merge_author_works(target: &[AuthorWork], source: &[AuthorWork]) -> Vec<AuthorWork> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for work in target.iter().chain(source.iter()) {
        let key = if work.work_key.trim().is_empty() {
            format!("t:{}", crate::author::normalize_title(&work.title))
        } else {
            format!("k:{}", work.work_key.trim())
        };
        if seen.insert(key) {
            out.push(work.clone());
        }
    }
    out
}

/// Fold two cached profiles into the target's identity, per
/// [`Catalog::merge_authors`].
fn merged_author_profile(
    source: Option<&AuthorProfile>,
    target: Option<&AuthorProfile>,
    source_display: &str,
    canonical: &str,
    target_key: &str,
) -> AuthorProfile {
    let mut profile = target
        .cloned()
        .or_else(|| source.cloned())
        .unwrap_or_default();
    let mut manual_fields: Vec<String> = match target {
        Some(target) => target.manual_fields.clone(),
        None => source
            .map(|s| {
                s.manual_fields
                    .iter()
                    .filter(|f| f.as_str() != AUTHOR_FIELD_NAME && f.as_str() != AUTHOR_FIELD_SORT_NAME)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut lock = |field: &str| {
        if !manual_fields.iter().any(|f| f == field) {
            manual_fields.push(field.to_string());
        }
    };

    if let (Some(source), Some(target)) = (source, target) {
        let source_locks = |field: &str| source.manual_fields.iter().any(|f| f == field);
        if profile.bio.trim().is_empty() && !source.bio.trim().is_empty() {
            profile.bio = source.bio.clone();
            if source_locks(AUTHOR_FIELD_BIO) {
                lock(AUTHOR_FIELD_BIO);
            }
        }
        if profile.birth_date.trim().is_empty() && !source.birth_date.trim().is_empty() {
            profile.birth_date = source.birth_date.clone();
            if source_locks(AUTHOR_FIELD_BIRTH_DATE) {
                lock(AUTHOR_FIELD_BIRTH_DATE);
            }
        }
        if profile.death_date.trim().is_empty() && !source.death_date.trim().is_empty() {
            profile.death_date = source.death_date.clone();
            if source_locks(AUTHOR_FIELD_DEATH_DATE) {
                lock(AUTHOR_FIELD_DEATH_DATE);
            }
        }
        if profile.photo_file.is_none() && source.photo_file.is_some() {
            profile.photo_file = source.photo_file.clone();
            profile.photo_path = source.photo_path.clone();
            if source_locks(AUTHOR_FIELD_PHOTO) {
                lock(AUTHOR_FIELD_PHOTO);
            }
        }
        if profile.top_work.trim().is_empty() {
            profile.top_work = source.top_work.clone();
        }
        if profile.openlibrary_key.trim().is_empty() {
            profile.openlibrary_key = source.openlibrary_key.clone();
        }
        if profile.source_url.trim().is_empty() {
            profile.source_url = source.source_url.clone();
        }
        let mut subjects = target.top_subjects.clone();
        for subject in &source.top_subjects {
            if !subjects
                .iter()
                .any(|s| s.eq_ignore_ascii_case(subject.trim()))
            {
                subjects.push(subject.clone());
            }
        }
        subjects.truncate(8);
        profile.top_subjects = subjects;
        profile.works = merge_author_works(&target.works, &source.works);
        profile.work_count = target
            .work_count
            .max(source.work_count)
            .max(profile.works.len() as i64);
        profile.fetched_at = [target.fetched_at.clone(), source.fetched_at.clone()]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .max()
            .unwrap_or_default();
        if source_locks(AUTHOR_FIELD_ALIASES) {
            lock(AUTHOR_FIELD_ALIASES);
        }
    }

    let sort_name = target
        .map(|t| t.sort_name.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::author::sort_author_name(canonical));

    let mut aliases: Vec<String> = Vec::new();
    if let Some(target) = target {
        aliases.extend(target.aliases.iter().cloned());
    }
    if let Some(source) = source {
        aliases.extend(source.aliases.iter().cloned());
        aliases.push(source.canonical_name.clone());
        aliases.push(source.sort_name.clone());
    }
    aliases.push(source_display.to_string());
    aliases.push(canonical.to_string());
    aliases.push(sort_name.clone());

    profile.canonical_name = canonical.to_string();
    profile.sort_name = sort_name;
    profile.normalized_name = target_key.to_string();
    profile.aliases = dedup_aliases(aliases);
    profile.manual_fields = manual_fields;
    profile.work_count = profile.work_count.max(profile.works.len() as i64);
    profile
}

fn json_vec_string(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn json_author_works(raw: &str) -> Vec<AuthorWork> {
    serde_json::from_str(raw).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_author_by_display_or_sort_name() {
        let catalog = Catalog::open_in_memory().unwrap();
        let profile = AuthorProfile {
            canonical_name: "George R. R. Martin".into(),
            sort_name: "Martin, George R. R.".into(),
            normalized_name: crate::author::normalize_author_name("George R. R. Martin"),
            aliases: vec!["George R.R. Martin".into()],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&profile).unwrap();

        let by_display = catalog
            .get_author_profile_by_name("George R. R. Martin")
            .unwrap()
            .unwrap();
        let by_sort = catalog
            .get_author_profile_by_name("Martin, George R. R.")
            .unwrap()
            .unwrap();

        assert_eq!(by_display.canonical_name, "George R. R. Martin");
        assert_eq!(by_sort.canonical_name, "George R. R. Martin");
    }

    #[test]
    fn books_for_author_includes_profile_aliases() {
        let catalog = Catalog::open_in_memory().unwrap();
        seed_book(&catalog, "alias-1", "Canonical Book", "J. K. Rowling", None);
        seed_book(&catalog, "alias-2", "Pen-name Book", "Robert Galbraith", None);
        seed_book(&catalog, "alias-3", "Other Book", "Another Author", None);
        let profile = AuthorProfile {
            canonical_name: "J. K. Rowling".into(),
            sort_name: "Rowling, J. K.".into(),
            normalized_name: crate::author::normalize_author_name("J. K. Rowling"),
            aliases: vec!["Robert Galbraith".into()],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&profile).unwrap();

        let books = catalog.books_for_author("J. K. Rowling").unwrap();
        assert_eq!(books.len(), 2);
        assert!(books.iter().any(|b| b.title == "Pen-name Book"));
    }

    #[test]
    fn books_for_author_queries_database_indexed() {
        use crate::models::BookFormat;
        let catalog = Catalog::open_in_memory().unwrap();
        catalog
            .insert_book(
                "uuid-1",
                "A Game of Thrones",
                "George R. R. Martin",
                Some("A Song of Ice and Fire"),
                "",
                BookFormat::Epub,
                "got.epub",
                "hash-1",
                None,
                &[],
            )
            .unwrap();
        catalog
            .insert_book(
                "uuid-2",
                "A Clash of Kings",
                "Martin, George R. R.",
                Some("A Song of Ice and Fire"),
                "",
                BookFormat::Epub,
                "cok.epub",
                "hash-2",
                None,
                &[],
            )
            .unwrap();
        catalog
            .insert_book(
                "uuid-3",
                "The Hobbit",
                "J. R. R. Tolkien",
                None,
                "",
                BookFormat::Epub,
                "hobbit.epub",
                "hash-3",
                None,
                &[],
            )
            .unwrap();

        let books = catalog.books_for_author("George R. R. Martin").unwrap();
        assert_eq!(books.len(), 2);
        let titles: Vec<_> = books.iter().map(|b| b.title.as_str()).collect();
        assert!(titles.contains(&"A Game of Thrones"));
        assert!(titles.contains(&"A Clash of Kings"));

        let books_sort = catalog.books_for_author("Martin, George R. R.").unwrap();
        assert_eq!(books_sort.len(), 2);
    }

    #[test]
    fn books_for_author_uses_minimal_queries() {
        use crate::models::BookFormat;
        let catalog = Catalog::open_in_memory().unwrap();
        for i in 0..20 {
            catalog
                .insert_book(
                    &format!("uuid-{i}"),
                    &format!("Book {i}"),
                    "Brandon Sanderson",
                    None,
                    "",
                    BookFormat::Epub,
                    &format!("book{i}.epub"),
                    &format!("hash-{i}"),
                    None,
                    &[],
                )
                .unwrap();
        }

        let query_count = catalog.count_queries(|| {
            let books = catalog.books_for_author("Brandon Sanderson").unwrap();
            assert_eq!(books.len(), 20);
        });

        assert!(query_count <= 5, "Expected <= 5 queries, got {query_count}");
    }

    fn seed_book(
        catalog: &Catalog,
        uuid: &str,
        title: &str,
        authors: &str,
        series: Option<&str>,
    ) -> i64 {
        use crate::models::BookFormat;
        catalog
            .insert_book(
                uuid,
                title,
                authors,
                series,
                "",
                BookFormat::Epub,
                &format!("{uuid}.epub"),
                &format!("hash-{uuid}"),
                None,
                &[],
            )
            .unwrap()
    }

    #[test]
    fn edit_author_renames_identity_and_locks_fields() {
        use crate::shelf_rules::{Rule, RuleField, RuleOp, RuleSet};

        let catalog = Catalog::open_in_memory().unwrap();
        let old = "George Raymond Martin";
        let new = "George R. R. Martin";
        let first = seed_book(
            &catalog,
            "edit-1",
            "A Game of Thrones",
            old,
            Some("A Song of Ice and Fire"),
        );
        let second = seed_book(
            &catalog,
            "edit-2",
            "A Shared Book",
            &format!("Someone Else; {old}"),
            None,
        );
        seed_book(&catalog, "edit-3", "The Hobbit", "J. R. R. Tolkien", None);

        let profile = AuthorProfile {
            canonical_name: old.into(),
            sort_name: "Martin, George Raymond".into(),
            normalized_name: crate::author::normalize_author_name(old),
            bio: "Fetched bio".into(),
            works: vec![AuthorWork {
                title: "A Game of Thrones".into(),
                work_key: "/works/OL1".into(),
                ..AuthorWork::default()
            }],
            aliases: vec!["Raymond Martin".into()],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&profile).unwrap();

        let rules = RuleSet {
            match_mode: "all".into(),
            rules: vec![Rule::new(RuleField::Author, RuleOp::Is, old)],
        };
        let shelf_id = catalog
            .create_shelf(old, ShelfKind::Smart, "", &rules.to_json())
            .unwrap();
        catalog.set_pref(
            &format!(
                "author.shelf.{}",
                crate::author::normalize_author_name(old)
            ),
            &format!("{shelf_id}:linked"),
        );
        let old_series_key = series_key("A Song of Ice and Fire", old);
        catalog
            .upsert_series_cache(
                &old_series_key,
                "test",
                &[SeriesWork {
                    title: "A Game of Thrones".into(),
                    key: "/works/OL1".into(),
                    cover_path: String::new(),
                    year: 1996,
                    author: old.into(),
                }],
            )
            .unwrap();

        let outcome = catalog
            .edit_author(
                old,
                &AuthorEdit {
                    name: new.into(),
                    sort_name: "Martin, George R. R.".into(),
                    bio: "Hand bio".into(),
                    birth_date: "20 September 1948".into(),
                    death_date: String::new(),
                    aliases: vec!["GRRM".into()],
                    photo: AuthorPhotoEdit::Replace("custom-martin.jpg".into()),
                },
            )
            .unwrap();

        assert_eq!(outcome.name, new);
        assert_eq!(outcome.books_updated, 2);
        assert_eq!(catalog.get_book(first).unwrap().unwrap().authors, new);
        assert_eq!(
            catalog.get_book(second).unwrap().unwrap().authors,
            format!("Someone Else; {new}")
        );

        let edited = catalog
            .get_author_profile_by_name("GRRM")
            .unwrap()
            .unwrap();
        assert_eq!(edited.canonical_name, new);
        assert_eq!(edited.bio, "Hand bio");
        assert_eq!(edited.photo_file.as_deref(), Some("custom-martin.jpg"));
        for field in [
            AUTHOR_FIELD_NAME,
            AUTHOR_FIELD_SORT_NAME,
            AUTHOR_FIELD_BIO,
            AUTHOR_FIELD_BIRTH_DATE,
            AUTHOR_FIELD_ALIASES,
            AUTHOR_FIELD_PHOTO,
        ] {
            assert!(edited.manual_fields.iter().any(|f| f == field), "missing lock {field}");
        }

        let shelf = catalog.get_shelf(shelf_id).unwrap().unwrap();
        assert_eq!(shelf.name, new);
        assert!(shelf.rules.contains(new));
        assert!(catalog
            .get_pref(&format!(
                "author.shelf.{}",
                crate::author::normalize_author_name(new)
            ))
            .is_some());
        assert!(catalog
            .get_pref(&format!(
                "author.shelf.{}",
                crate::author::normalize_author_name(old)
            ))
            .is_none());

        let cached = catalog
            .get_cached_series(&series_key("A Song of Ice and Fire", new))
            .unwrap()
            .unwrap();
        assert_eq!(cached.works[0].author, new);

        // A later fetch may refresh the bibliography, but the
        // hand-edited fields stay (decision 48).
        let fetched = AuthorProfile {
            canonical_name: "George Martin".into(),
            sort_name: "Martin, George".into(),
            normalized_name: crate::author::normalize_author_name(new),
            bio: "Fetched replacement bio".into(),
            birth_date: "1900".into(),
            photo_file: Some("fetched.jpg".into()),
            works: edited.works.clone(),
            aliases: vec!["George Martin".into()],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&fetched).unwrap();
        let after_fetch = catalog
            .get_author_profile_by_name(new)
            .unwrap()
            .unwrap();
        assert_eq!(after_fetch.canonical_name, new);
        assert_eq!(after_fetch.sort_name, "Martin, George R. R.");
        assert_eq!(after_fetch.bio, "Hand bio");
        assert_eq!(after_fetch.birth_date, "20 September 1948");
        assert_eq!(after_fetch.photo_file.as_deref(), Some("custom-martin.jpg"));
        assert!(after_fetch.aliases.iter().any(|a| a == "GRRM"));
    }

    #[test]
    fn edit_author_refuses_an_existing_author() {
        let catalog = Catalog::open_in_memory().unwrap();
        let first = seed_book(&catalog, "edit-x1", "One", "Author One", None);
        seed_book(&catalog, "edit-x2", "Two", "Author Two", None);

        let result = catalog.edit_author(
            "Author One",
            &AuthorEdit {
                name: "Author Two".into(),
                ..AuthorEdit::default()
            },
        );
        assert!(result.is_err());
        assert_eq!(
            catalog.get_book(first).unwrap().unwrap().authors,
            "Author One"
        );
    }

    #[test]
    fn merge_authors_moves_books_profiles_and_comic_authors() {
        let catalog = Catalog::open_in_memory().unwrap();
        let source = "Robert Galbraith";
        let target = "J. K. Rowling";
        seed_book(&catalog, "merge-1", "The Cuckoo's Calling", source, None);
        let shared = seed_book(
            &catalog,
            "merge-2",
            "Shared Credit",
            &format!("{target}; {source}"),
            None,
        );
        seed_book(&catalog, "merge-3", "Harry Potter", target, None);
        let comic_id = catalog
            .get_or_create_comic_series("Pen-name Comics", Some(source), None)
            .unwrap();

        let source_profile = AuthorProfile {
            canonical_name: source.into(),
            sort_name: "Galbraith, Robert".into(),
            normalized_name: crate::author::normalize_author_name(source),
            bio: "Hand-written pen-name bio".into(),
            photo_file: Some("pen.jpg".into()),
            works: vec![
                AuthorWork {
                    title: "Duplicate".into(),
                    work_key: "/works/OL1".into(),
                    ..AuthorWork::default()
                },
                AuthorWork {
                    title: "Source Only".into(),
                    work_key: "/works/OL3".into(),
                    ..AuthorWork::default()
                },
            ],
            aliases: vec!["R. Galbraith".into()],
            manual_fields: vec![AUTHOR_FIELD_BIO.into(), AUTHOR_FIELD_PHOTO.into()],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&source_profile).unwrap();
        let target_profile = AuthorProfile {
            canonical_name: target.into(),
            sort_name: "Rowling, J. K.".into(),
            normalized_name: crate::author::normalize_author_name(target),
            works: vec![
                AuthorWork {
                    title: "Duplicate".into(),
                    work_key: "/works/OL1".into(),
                    ..AuthorWork::default()
                },
                AuthorWork {
                    title: "Target Only".into(),
                    work_key: "/works/OL2".into(),
                    ..AuthorWork::default()
                },
            ],
            ..AuthorProfile::default()
        };
        catalog.upsert_author_profile(&target_profile).unwrap();

        let outcome = catalog.merge_authors(source, target).unwrap();
        assert_eq!(outcome.name, target);
        assert_eq!(outcome.books_updated, 2);
        assert_eq!(catalog.get_book(shared).unwrap().unwrap().authors, target);
        assert_eq!(catalog.books_for_author(target).unwrap().len(), 3);
        assert_eq!(
            catalog
                .get_comic_series_by_title("Pen-name Comics")
                .unwrap()
                .unwrap()
                .id,
            comic_id
        );
        assert_eq!(
            catalog
                .get_comic_series_by_title("Pen-name Comics")
                .unwrap()
                .unwrap()
                .author,
            target
        );

        let merged = catalog
            .get_author_profile_by_name(source)
            .unwrap()
            .unwrap();
        assert_eq!(merged.canonical_name, target);
        assert_eq!(merged.bio, "Hand-written pen-name bio");
        assert_eq!(merged.photo_file.as_deref(), Some("pen.jpg"));
        assert_eq!(merged.works.len(), 3);
        assert!(merged.aliases.iter().any(|a| a == source));
        assert!(merged.aliases.iter().any(|a| a == "R. Galbraith"));
        assert!(merged
            .manual_fields
            .iter()
            .any(|f| f == AUTHOR_FIELD_BIO));
        assert_eq!(catalog.all_author_profiles().unwrap().len(), 1);
    }
}
