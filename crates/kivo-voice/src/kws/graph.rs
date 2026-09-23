//! The keyword graph: an Aho–Corasick trie over each keyword's tokens, as in sherpa-onnx's
//! `ContextGraph`. Following a keyword's tokens earns its boost score (so the beam search keeps
//! paths that spell a keyword), falling off a keyword takes the score back, and reaching a
//! keyword's end reports it together with its acoustic threshold.

use std::collections::{HashMap, VecDeque};

/// A state in the graph (index into `ContextGraph::states`).
pub type StateId = usize;
pub const ROOT: StateId = 0;

#[derive(Debug)]
pub struct State {
    pub token: i64,
    token_score: f32,
    node_score: f32,
    output_score: f32,
    /// How many tokens deep this state is.
    pub level: usize,
    pub threshold: f32,
    pub is_end: bool,
    /// The keyword ending here (its caller's id).
    pub keyword: Option<usize>,
    next: HashMap<i64, StateId>,
    fail: StateId,
    output: Option<StateId>,
}

impl State {
    fn new(token: i64, level: usize) -> Self {
        Self {
            token,
            token_score: 0.0,
            node_score: 0.0,
            output_score: 0.0,
            level,
            threshold: 0.0,
            is_end: false,
            keyword: None,
            next: HashMap::new(),
            fail: ROOT,
            output: None,
        }
    }
}

/// One keyword to watch for.
#[derive(Clone, Debug)]
pub struct Entry {
    pub tokens: Vec<i64>,
    /// Added to a path's score per matching token (sherpa's default: 1.0).
    pub boost: f32,
    /// The average token probability a match needs (sherpa's default: 0.25).
    pub threshold: f32,
}

pub struct ContextGraph {
    pub states: Vec<State>,
}

impl ContextGraph {
    pub fn new(entries: &[Entry]) -> Self {
        let mut states = vec![State::new(-1, 0)];
        for (index, entry) in entries.iter().enumerate() {
            let mut node = ROOT;
            let last = entry.tokens.len().saturating_sub(1);
            for (j, &token) in entry.tokens.iter().enumerate() {
                let is_end = j == last;
                let parent_score = states[node].node_score;
                let child = if let Some(&child) = states[node].next.get(&token) {
                    let c = &mut states[child];
                    c.token_score = c.token_score.max(entry.boost);
                    c.node_score = parent_score + c.token_score;
                    c.is_end = c.is_end || is_end;
                    c.output_score = if c.is_end { c.node_score } else { 0.0 };
                    if is_end {
                        c.keyword = Some(index);
                        c.threshold = entry.threshold;
                    }
                    child
                } else {
                    let mut c = State::new(token, j + 1);
                    c.token_score = entry.boost;
                    c.node_score = parent_score + entry.boost;
                    if is_end {
                        c.is_end = true;
                        c.output_score = c.node_score;
                        c.threshold = entry.threshold;
                        c.keyword = Some(index);
                    }
                    states.push(c);
                    let id = states.len() - 1;
                    states[node].next.insert(token, id);
                    id
                };
                node = child;
            }
        }
        let mut graph = Self { states };
        graph.fill_fail_and_output();
        graph
    }

    /// Follows fail links from `from` until a state has an arc for `token` (or the root).
    fn fall_back(&self, from: StateId, token: i64) -> StateId {
        let mut node = from;
        while !self.states[node].next.contains_key(&token) {
            node = self.states[node].fail;
            if node == ROOT {
                break;
            }
        }
        self.states[node].next.get(&token).copied().unwrap_or(node)
    }

    fn fill_fail_and_output(&mut self) {
        let mut queue: VecDeque<StateId> = self.states[ROOT].next.values().copied().collect();
        for &child in &queue {
            self.states[child].fail = ROOT;
        }
        while let Some(current) = queue.pop_front() {
            let children: Vec<(i64, StateId)> = self.states[current]
                .next
                .iter()
                .map(|(&t, &c)| (t, c))
                .collect();
            for (token, child) in children {
                let parent_fail = self.states[current].fail;
                let fail = if let Some(&f) = self.states[parent_fail].next.get(&token) {
                    f
                } else {
                    self.fall_back(self.states[parent_fail].fail, token)
                };
                self.states[child].fail = fail;
                // The nearest keyword end along the fail chain (none once it reaches the root).
                let mut o = fail;
                let output = loop {
                    if self.states[o].is_end {
                        break Some(o);
                    }
                    o = self.states[o].fail;
                    if o == ROOT {
                        break None;
                    }
                };
                self.states[child].output = output;
                if let Some(o) = output {
                    self.states[child].output_score += self.states[o].output_score;
                }
                queue.push_back(child);
            }
        }
    }

    /// Takes `token` from `state`: the score to add and the next state (sherpa's strict mode).
    pub fn step(&self, state: StateId, token: i64) -> (f32, StateId) {
        let here = &self.states[state];
        let (node, score) = if let Some(&next) = here.next.get(&token) {
            (next, self.states[next].token_score)
        } else {
            let node = self.fall_back(here.fail, token);
            (node, self.states[node].node_score - here.node_score)
        };
        (score + self.states[node].output_score, node)
    }

    /// The keyword state `state` completes, if any.
    pub fn matched(&self, state: StateId) -> Option<StateId> {
        let s = &self.states[state];
        if s.is_end { Some(state) } else { s.output }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tokens: &[i64]) -> Entry {
        Entry {
            tokens: tokens.to_vec(),
            boost: 1.0,
            threshold: 0.25,
        }
    }

    #[test]
    fn following_a_keyword_earns_its_boost_and_reports_the_end() {
        let g = ContextGraph::new(&[entry(&[5, 6, 7]), entry(&[6, 8])]);
        let (s1, a) = g.step(ROOT, 5);
        let (s2, b) = g.step(a, 6);
        let (s3, c) = g.step(b, 7);
        assert_eq!((s1, s2), (1.0, 1.0));
        // Reaching the end adds the output score (the whole path's score).
        assert!((s3 - 4.0).abs() < 1e-6, "{s3}");
        let end = g.matched(c).unwrap();
        assert_eq!((g.states[end].keyword, g.states[end].level), (Some(0), 3));
        assert!(g.matched(b).is_none());
    }

    #[test]
    fn falling_off_a_keyword_takes_the_boost_back() {
        let g = ContextGraph::new(&[entry(&[5, 6, 7])]);
        let (_, a) = g.step(ROOT, 5);
        let (_, b) = g.step(a, 6);
        let (score, back) = g.step(b, 9);
        assert_eq!(back, ROOT);
        assert!((score + 2.0).abs() < 1e-6, "{score}");
    }

    #[test]
    fn a_keyword_inside_another_path_is_found_by_its_fail_links() {
        // "6 8" is reached from inside "5 6 …": after 5, 6 the next token 8 jumps across.
        let g = ContextGraph::new(&[entry(&[5, 6, 7]), entry(&[6, 8])]);
        let (_, a) = g.step(ROOT, 5);
        let (_, b) = g.step(a, 6);
        let (_, c) = g.step(b, 8);
        let end = g.matched(c).unwrap();
        assert_eq!(g.states[end].keyword, Some(1));
    }
}
