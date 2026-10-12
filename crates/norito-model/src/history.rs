//! Bounded snapshot history. Knows nothing about what a snapshot contains.

/// Full snapshots for this prototype; transactions will replace these.
const LIMIT: usize = 100;

#[derive(Debug, Clone)]
pub(crate) struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            undo: vec![],
            redo: vec![],
        }
    }
}

impl<T> History<T> {
    /// Record `previous` as the state before a new edit. Clears redo.
    pub(crate) fn record(&mut self, previous: T) {
        self.undo.push(previous);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    /// Swap `current` with the last undo state.
    pub(crate) fn undo(&mut self, current: &mut T) -> bool {
        Self::step(&mut self.undo, &mut self.redo, current)
    }
    /// Swap `current` with the last redo state.
    pub(crate) fn redo(&mut self, current: &mut T) -> bool {
        Self::step(&mut self.redo, &mut self.undo, current)
    }
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    fn step(from: &mut Vec<T>, to: &mut Vec<T>, current: &mut T) -> bool {
        let Some(state) = from.pop() else {
            return false;
        };
        to.push(std::mem::replace(current, state));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_redo_cleared_by_new_edits() {
        let mut history = History::default();
        let mut current = 0;
        for next in 1..=150 {
            history.record(std::mem::replace(&mut current, next));
        }
        let mut steps = 0;
        while history.undo(&mut current) {
            steps += 1;
        }
        assert_eq!((steps, current), (LIMIT, 50));
        assert!(history.redo(&mut current));
        history.record(std::mem::replace(&mut current, -1));
        assert!(!history.can_redo());
    }
}
