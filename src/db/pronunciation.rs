//! Offline pronunciation for the dictionary popup.
//!
//! The CMU Pronouncing Dictionary (cmudict 0.7a, BSD-style license — see
//! `resources/dictionaries/cmudict-0.7a.NOTICE.txt`) is packed as a
//! gzipped `word\tARPABET` TSV and parsed lazily into memory. ARPABET is
//! converted to a compact IPA transcription for display ("B AE1 NG K" →
//! `/ˈbæŋk/`). Fully offline: the packed file is compiled into the binary,
//! so a lookup never touches the network.

use super::dictionaries::fold_key;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The packed CMU Pronouncing Dictionary: word → ARPABET phonemes, one row
/// per pronunciation variant in counter order.
const CMUDICT_GZ: &[u8] = include_bytes!("../../resources/dictionaries/cmudict-0.7a.tsv.gz");

/// One ARPABET phoneme, decoded: whether it is a vowel, its IPA glyphs and
/// its stress digit (0/1/2, or `None` for consonants and unstressed
/// vowels). `None` for an unknown phoneme.
fn arpabet_phoneme(phoneme: &str) -> Option<(bool, &'static str, Option<u8>)> {
    let bytes = phoneme.as_bytes();
    let (core, stress) = match bytes.split_last() {
        Some((last, rest)) if last.is_ascii_digit() => (&phoneme[..rest.len()], Some(*last)),
        _ => (phoneme, None),
    };
    let (vowel, glyph) = match core {
        // Vowels
        "AA" => (true, "ɑ"),
        "AE" => (true, "æ"),
        // Unstressed AH is the schwa (hello → /həˈloʊ/); stressed AH is the
        // STRUT vowel (run → /ˈrʌn/).
        "AH" if stress == Some(b'0') => (true, "ə"),
        "AH" => (true, "ʌ"),
        "AO" => (true, "ɔ"),
        "AW" => (true, "aʊ"),
        "AY" => (true, "aɪ"),
        "EH" => (true, "ɛ"),
        "ER" => (true, "ɚ"),
        "EY" => (true, "eɪ"),
        "IH" => (true, "ɪ"),
        "IY" => (true, "i"),
        "OW" => (true, "oʊ"),
        "OY" => (true, "ɔɪ"),
        "UH" => (true, "ʊ"),
        "UW" => (true, "u"),
        // Consonants
        "B" => (false, "b"),
        "CH" => (false, "tʃ"),
        "D" => (false, "d"),
        "DH" => (false, "ð"),
        "F" => (false, "f"),
        "G" => (false, "ɡ"),
        "HH" => (false, "h"),
        "JH" => (false, "dʒ"),
        "K" => (false, "k"),
        "L" => (false, "l"),
        "M" => (false, "m"),
        "N" => (false, "n"),
        "NG" => (false, "ŋ"),
        "P" => (false, "p"),
        "R" => (false, "r"),
        "S" => (false, "s"),
        "SH" => (false, "ʃ"),
        "T" => (false, "t"),
        "TH" => (false, "θ"),
        "V" => (false, "v"),
        "W" => (false, "w"),
        "Y" => (false, "j"),
        "Z" => (false, "z"),
        "ZH" => (false, "ʒ"),
        _ => return None,
    };
    Some((vowel, glyph, stress))
}

/// Convert an ARPABET transcription to IPA ("B AE1 NG K" → "ˈbæŋk").
///
/// A stress mark is placed before the syllable's onset — the consonants
/// preceding its vowel — per IPA convention ("bæŋk" → "ˈbæŋk", not
/// "bˈæŋk"). ARPABET carries no syllable boundaries, so an ambisyllabic
/// consonant stays with the stressed syllable ("accent" → "ˌæˈksɛnt");
/// this is the standard simplification of ARPABET-to-IPA converters.
/// Unknown phonemes are skipped.
pub fn arpabet_to_ipa(phonemes: &str) -> String {
    let mut out = String::new();
    let mut onset = String::new();
    for phoneme in phonemes.split_whitespace() {
        let Some((is_vowel, glyph, stress)) = arpabet_phoneme(phoneme) else {
            continue;
        };
        if is_vowel {
            match stress {
                Some(b'1') => out.push('ˈ'),
                Some(b'2') => out.push('ˌ'),
                _ => {}
            }
            out.push_str(&onset);
            out.push_str(glyph);
            onset.clear();
        } else {
            onset.push_str(glyph);
        }
    }
    out.push_str(&onset);
    out
}

/// The pronunciation table, parsed once: folded key → IPA variants in
/// counter order. Loaded lazily — a lookup that never opens the dictionary
/// popup never pays the parse.
fn cmudict_table() -> &'static HashMap<String, Vec<String>> {
    static TABLE: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        let decoder = flate2::read::GzDecoder::new(CMUDICT_GZ);
        let reader = std::io::BufReader::new(decoder);
        for line in std::io::BufRead::lines(reader).map_while(|line| line.ok()) {
            let Some((word, phonemes)) = line.split_once('\t') else {
                continue;
            };
            let ipa = arpabet_to_ipa(phonemes);
            if ipa.is_empty() {
                continue;
            }
            map.entry(fold_key(word)).or_default().push(ipa);
        }
        map
    })
}

/// The most likely IPA transcription of a word, or `None` when the packed
/// CMU dictionary has no entry. Falls back through the same query variants
/// as dictionary lookups (inflections and exception-list lemmas), so
/// "running" resolves even though only "run" is listed, and the fold key
/// makes it case- and diacritic-insensitive ("Rún" finds "run").
pub fn pronunciation_for(word: &str) -> Option<String> {
    let table = cmudict_table();
    if let Some(variants) = table.get(&fold_key(word)) {
        return variants.first().cloned();
    }
    for variant in super::dictionaries::dictionary_query_variants(word) {
        if let Some(variants) = table.get(&fold_key(&variant)) {
            return variants.first().cloned();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arpabet_to_ipa_converts_common_words() {
        assert_eq!(arpabet_to_ipa("B AE1 NG K"), "ˈbæŋk");
        assert_eq!(arpabet_to_ipa("DH AH0"), "ðə");
        assert_eq!(arpabet_to_ipa("HH AH0 L OW1"), "həˈloʊ");
        assert_eq!(arpabet_to_ipa("S EH1 T"), "ˈsɛt");
        assert_eq!(arpabet_to_ipa("L AY1 T"), "ˈlaɪt");
        assert_eq!(arpabet_to_ipa("R AH1 N IH0 NG"), "ˈrʌnɪŋ");
    }

    #[test]
    fn arpabet_to_ipa_marks_secondary_stress() {
        // The ambisyllabic K stays with the stressed syllable (ARPABET has
        // no syllable boundaries) — the documented simplification.
        assert_eq!(arpabet_to_ipa("AE2 K S EH1 N T"), "ˌæˈksɛnt");
        assert_eq!(arpabet_to_ipa("T OY1"), "ˈtɔɪ");
    }

    #[test]
    fn arpabet_to_ipa_skips_unknown_phonemes() {
        assert_eq!(arpabet_to_ipa("B AE1 XX NG K"), "ˈbæŋk");
        assert_eq!(arpabet_to_ipa(""), "");
    }

    #[test]
    fn pronunciation_for_hits_common_words() {
        assert_eq!(pronunciation_for("bank"), Some("ˈbæŋk".to_string()));
        assert_eq!(pronunciation_for("the"), Some("ðə".to_string()));
        assert_eq!(pronunciation_for("run"), Some("ˈrʌn".to_string()));
    }

    #[test]
    fn pronunciation_for_is_fold_case_and_diacritic_insensitive() {
        assert_eq!(pronunciation_for("BANK"), Some("ˈbæŋk".to_string()));
        assert_eq!(pronunciation_for("Rún"), Some("ˈrʌn".to_string()));
    }

    #[test]
    fn pronunciation_for_handles_inflected_forms() {
        // cmudict lists inflected forms directly ("running", "banks"),
        // each with its own transcription.
        assert_eq!(pronunciation_for("running"), Some("ˈrʌnɪŋ".to_string()));
        assert_eq!(pronunciation_for("banks"), Some("ˈbæŋks".to_string()));
    }

    #[test]
    fn pronunciation_for_unknown_word_returns_none() {
        assert_eq!(pronunciation_for("zzzzqwqx"), None);
        assert_eq!(pronunciation_for(""), None);
    }

    #[test]
    fn packed_table_has_reasonable_coverage() {
        // Sanity-check the packed resource itself: well over 100k words.
        let table = cmudict_table();
        assert!(table.len() > 100_000, "table has {} entries", table.len());
        // The first variant of a multi-pronunciation word is preferred.
        assert_eq!(pronunciation_for("either"), Some("ˈiðɚ".to_string()));
    }
}
