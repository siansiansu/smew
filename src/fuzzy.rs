//! A small fzf-style fuzzy matcher for pickers (profiles, command
//! suggestions). Subsequence matching with a simple score: consecutive
//! matches and word-boundary hits rank higher, gaps rank lower. No crate —
//! the needs are tiny and the scoring stays predictable/tunable.

/// A successful match: higher score = better; `positions` are the haystack
/// char indexes that matched (for highlighting).
#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyMatch {
    pub score: i32,
    pub positions: Vec<usize>,
}

const SCORE_MATCH: i32 = 4; // per matched char
const BONUS_CONSECUTIVE: i32 = 10; // adjacent to the previous match
const BONUS_BOUNDARY: i32 = 8; // start of string or after -_./ space
const BONUS_CASE: i32 = 1; // exact-case hit on an uppercase query char
const PENALTY_GAP: i32 = 1; // per skipped haystack char between hits

/// Matches `query` as a case-insensitive subsequence of `hay`, choosing the
/// highest-scoring alignment (small DP — inputs are short picker strings).
/// Returns None when any query char is missing. An empty query matches
/// everything with score 0.
pub fn fuzzy_match(query: &str, hay: &str) -> Option<FuzzyMatch> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            positions: Vec::new(),
        });
    }
    let h: Vec<char> = hay.chars().collect();
    let hl: Vec<char> = h
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let (m, n) = (q.len(), h.len());
    if m > n {
        return None;
    }

    // score[i][j]: best score matching q[..=i] with q[i] matched at h[j].
    const MISS: i32 = i32::MIN / 2;
    let mut score = vec![vec![MISS; n]; m];
    let mut parent = vec![vec![usize::MAX; n]; m];
    for (i, &qc) in q.iter().enumerate() {
        let qlc = qc.to_lowercase().next().unwrap_or(qc);
        for j in i..n {
            if hl[j] != qlc {
                continue;
            }
            let boundary = j == 0 || matches!(h[j - 1], '-' | '_' | '.' | '/' | ' ');
            let mut best = SCORE_MATCH
                + if boundary { BONUS_BOUNDARY } else { 0 }
                + if h[j] == qc && qc.is_uppercase() {
                    BONUS_CASE
                } else {
                    0
                };
            if i == 0 {
                // leading skipped chars count as a gap
                score[i][j] = best - PENALTY_GAP * j as i32;
                continue;
            }
            let mut from = usize::MAX;
            let mut with_prev = MISS;
            for (p, &prev) in score[i - 1].iter().enumerate().take(j) {
                if prev == MISS {
                    continue;
                }
                let consec = if p + 1 == j { BONUS_CONSECUTIVE } else { 0 };
                let gap = PENALTY_GAP * (j - p - 1) as i32;
                let cand = prev + consec - gap;
                if cand > with_prev {
                    with_prev = cand;
                    from = p;
                }
            }
            if from == usize::MAX {
                continue;
            }
            best += with_prev;
            score[i][j] = best;
            parent[i][j] = from;
        }
    }

    let (mut j, &total) = score[m - 1]
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .filter(|(_, s)| **s != MISS)?;
    let mut positions = vec![0usize; m];
    for i in (0..m).rev() {
        positions[i] = j;
        j = parent[i][j];
    }
    // Shorter haystacks win ties ("dev" should beat "dev-staging" for "dev").
    let total = total - (n as i32 - m as i32).max(0) / 4;
    Some(FuzzyMatch {
        score: total,
        positions,
    })
}

/// Filters + sorts `items` by fuzzy score (desc), stable on the original
/// order for equal scores. Returns (index into items, match).
pub fn rank<'a, I>(query: &str, items: I) -> Vec<(usize, FuzzyMatch)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out: Vec<(usize, FuzzyMatch)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(i, s)| fuzzy_match(query, s).map(|fm| (i, fm)))
        .collect();
    out.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_all() {
        assert_eq!(fuzzy_match("", "anything").unwrap().score, 0);
        assert!(fuzzy_match("", "").is_some());
    }

    #[test]
    fn subsequence_and_miss() {
        assert!(fuzzy_match("cldprd", "Cloud.prod").is_some());
        assert!(fuzzy_match("xyz", "Cloud.prod").is_none());
        // all query chars must appear, in order
        assert!(fuzzy_match("dorp", "Cloud.prod").is_none());
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_match("CLOUD", "cloud.dev").is_some());
        assert!(fuzzy_match("cloud", "CLOUD.DEV").is_some());
    }

    #[test]
    fn consecutive_beats_scattered() {
        let a = fuzzy_match("prod", "Cloud.prod").unwrap().score;
        let b = fuzzy_match("prod", "p-r-o-d-x").unwrap().score;
        assert!(a > b, "consecutive {a} must beat scattered {b}");
    }

    #[test]
    fn boundary_beats_middle() {
        let a = fuzzy_match("dev", "cloud-dev").unwrap().score;
        let b = fuzzy_match("dev", "cloudev").unwrap().score;
        assert!(a > b, "boundary {a} must beat mid-word {b}");
    }

    #[test]
    fn exact_beats_longer() {
        let names = ["dev-staging", "dev", "developer"];
        let ranked = rank("dev", names);
        assert_eq!(ranked[0].0, 1, "exact 'dev' must rank first: {ranked:?}");
    }

    #[test]
    fn positions_point_at_matches() {
        let m = fuzzy_match("cp", "Cloud.prod").unwrap();
        assert_eq!(m.positions, vec![0, 6]);
    }

    #[test]
    fn rank_filters_and_sorts() {
        let names = ["personal", "Cloud.dev", "Cloud.prod"];
        let ranked = rank("cloud", names);
        assert_eq!(ranked.len(), 2);
        assert!(ranked.iter().all(|(i, _)| *i != 0));
        // whitespace in the query is ignored (fzf-style)
        assert_eq!(rank("clo ud", names).len(), 2);
    }
}
