//! Line-level diff for the editor's Compare view (decision 134).
//! Pure text work, like the other editor engines: given the
//! snapshot's text and the current text, produce the aligned
//! two-column rows the side-by-side view renders.
//!
//! The algorithm is Myers' greedy O(ND) search with the classic
//! V-array trace, after trimming the common prefix and suffix —
//! which for edited book files (one chapter touched in the
//! middle) collapses the search to almost nothing.

/// One aligned pair of lines: the row both columns render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// The two lines are identical.
    Same,
    /// The lines form an edited pair (a deletion opposite an
    /// insertion), or an unmatched removed line.
    Removed,
    /// An unmatched added line.
    Added,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedRow {
    /// Line index into the old text, when the row has one.
    pub old: Option<usize>,
    /// Line index into the new text, when the row has one.
    pub new: Option<usize>,
    pub kind: RowKind,
}

/// Split as the diff sees lines: a trailing newline yields a
/// final empty line, identically on both sides.
pub fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal,
    Delete,
    Insert,
}

/// Diff `old` against `new` and align the operations into rows
/// for a two-column render. Returns the rows plus the number of
/// change hunks (maximal runs of non-equal rows) — the header's
/// "1 change".
pub fn align(old: &str, new: &str) -> (Vec<AlignedRow>, usize) {
    let a = lines_of(old);
    let b = lines_of(new);
    let ops = diff_ops(&a, &b);

    let mut rows: Vec<AlignedRow> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut k = 0usize;
    while k < ops.len() {
        match ops[k] {
            Op::Equal => {
                rows.push(AlignedRow {
                    old: Some(i),
                    new: Some(j),
                    kind: RowKind::Same,
                });
                i += 1;
                j += 1;
                k += 1;
            }
            Op::Delete | Op::Insert => {
                let mut dels = 0usize;
                let mut inss = 0usize;
                while k < ops.len() && ops[k] != Op::Equal {
                    match ops[k] {
                        Op::Delete => dels += 1,
                        Op::Insert => inss += 1,
                        Op::Equal => {}
                    }
                    k += 1;
                }
                // Pair the runs up: line against line reads as
                // an edit; the overflow is a pure removal/add.
                let paired = dels.min(inss);
                for p in 0..paired {
                    rows.push(AlignedRow {
                        old: Some(i + p),
                        new: Some(j + p),
                        kind: RowKind::Removed,
                    });
                }
                for p in paired..dels {
                    rows.push(AlignedRow {
                        old: Some(i + p),
                        new: None,
                        kind: RowKind::Removed,
                    });
                }
                for p in paired..inss {
                    rows.push(AlignedRow {
                        old: None,
                        new: Some(j + p),
                        kind: RowKind::Added,
                    });
                }
                i += dels;
                j += inss;
            }
        }
    }

    let mut hunks = 0usize;
    let mut in_hunk = false;
    for row in &rows {
        if row.kind == RowKind::Same {
            in_hunk = false;
        } else if !in_hunk {
            hunks += 1;
            in_hunk = true;
        }
    }
    (rows, hunks)
}

/// The raw edit script. Common affixes are peeled first; the
/// middle goes through Myers' search.
fn diff_ops(a: &[&str], b: &[&str]) -> Vec<Op> {
    let mut prefix = 0usize;
    while prefix < a.len().min(b.len()) && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < (a.len() - prefix).min(b.len() - prefix)
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let mid_a = &a[prefix..a.len() - suffix];
    let mid_b = &b[prefix..b.len() - suffix];

    let mut ops = vec![Op::Equal; prefix];
    ops.extend(myers(mid_a, mid_b));
    ops.extend(vec![Op::Equal; suffix]);
    ops
}

/// Myers' greedy algorithm with V-array snapshots for the
/// backtrack. `a` lines delete downward, `b` lines insert
/// rightward, matches ride the diagonals.
fn myers(a: &[&str], b: &[&str]) -> Vec<Op> {
    let (n, m) = (a.len() as i32, b.len() as i32);
    if n == 0 {
        return vec![Op::Insert; m as usize];
    }
    if m == 0 {
        return vec![Op::Delete; n as usize];
    }
    let max = n + m;
    let span = (2 * max + 1) as usize;
    let idx = |k: i32| (k + max) as usize;
    let mut v = vec![0i32; span];
    let mut trace: Vec<Vec<i32>> = Vec::new();

    let mut found = None;
    'search: for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[idx(k - 1)] < v[idx(k + 1)]) {
                v[idx(k + 1)]
            } else {
                v[idx(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx(k)] = x;
            if x >= n && y >= m {
                found = Some(d);
                break 'search;
            }
            k += 2;
        }
    }
    let Some(mut d) = found else {
        // Unreachable (D = n + m always reaches the corner), but
        // a total rewrite beats a panic on hostile input.
        let mut ops = vec![Op::Delete; n as usize];
        ops.extend(vec![Op::Insert; m as usize]);
        return ops;
    };

    // Backtrack from the corner, collecting the script reversed.
    let (mut x, mut y) = (n, m);
    let mut rev: Vec<Op> = Vec::new();
    while d > 0 {
        let prior = &trace[d as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && prior[idx(k - 1)] < prior[idx(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = prior[idx(prev_k)];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            rev.push(Op::Equal);
            x -= 1;
            y -= 1;
        }
        if x == prev_x {
            rev.push(Op::Insert);
            y -= 1;
        } else {
            rev.push(Op::Delete);
            x -= 1;
        }
        d -= 1;
    }
    while x > 0 && y > 0 {
        rev.push(Op::Equal);
        x -= 1;
        y -= 1;
    }
    rev.reverse();
    rev
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(rows: &[AlignedRow]) -> Vec<RowKind> {
        rows.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn identical_text_is_all_same() {
        let (rows, hunks) = align("a\nb\nc", "a\nb\nc");
        assert_eq!(hunks, 0);
        assert!(rows.iter().all(|r| r.kind == RowKind::Same));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn edited_line_pairs_up() {
        let (rows, hunks) = align(
            "<p>She counted the lamps</p>\n<p>The road bent north past the mill.</p>\n<p>Keep to the left.</p>",
            "<p>She counted the lamps</p>\n<p>The road bent north, past the old mill.</p>\n<p>Keep to the left.</p>",
        );
        assert_eq!(hunks, 1);
        assert_eq!(
            kinds(&rows),
            vec![RowKind::Same, RowKind::Removed, RowKind::Same]
        );
        assert_eq!(rows[1].old, Some(1));
        assert_eq!(rows[1].new, Some(1));
    }

    #[test]
    fn insertion_and_deletion_pad_the_other_side() {
        let (rows, hunks) = align("a\nb\nc\nd", "a\nc\nd\ne\nf");
        assert_eq!(hunks, 2);
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::Same,
                RowKind::Removed,
                RowKind::Same,
                RowKind::Same,
                RowKind::Added,
                RowKind::Added,
            ]
        );
        assert_eq!(rows[1].new, None, "the deleted line faces a gap");
        assert_eq!(rows[4].old, None, "an added line faces a gap");
        assert_eq!(rows[5].new, Some(4));
    }

    #[test]
    fn whole_file_rewrite_still_aligns() {
        let (rows, hunks) = align("x\ny", "p\nq\nr");
        assert_eq!(hunks, 1);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].kind, RowKind::Added);
    }

    #[test]
    fn script_reconstructs_both_sides() {
        let old = "one\ntwo\nthree\nfour\nfive\nsix";
        let new = "one\nTWO\nthree\nfive\nsix\nseven";
        let (rows, _) = align(old, new);
        let a = lines_of(old);
        let b = lines_of(new);
        let got_old: Vec<&str> = rows.iter().filter_map(|r| r.old.map(|i| a[i])).collect();
        let got_new: Vec<&str> = rows.iter().filter_map(|r| r.new.map(|i| b[i])).collect();
        assert_eq!(got_old, a);
        assert_eq!(got_new, b);
    }
}
