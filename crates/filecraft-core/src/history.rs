//! Session undo and redo over state snapshots: at most ten undo steps, and any change clears the
//! redo steps.

pub const MAX_HISTORY: usize = 10;

#[derive(Debug, Clone)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    /// The last observed state, used to detect changes.
    current: T,
    has_unsaved_changes: bool,
}

impl<T: Clone> History<T> {
    pub fn new(current: T) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            current,
            has_unsaved_changes: false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.has_unsaved_changes
    }

    pub fn mark_saved(&mut self) {
        self.has_unsaved_changes = false;
    }

    /// Compares `state` with the last observed state. When `same` reports a difference, the
    /// previous state becomes an undo step. The observed state is updated either way.
    pub fn observe(&mut self, state: T, same: impl Fn(&T, &T) -> bool) {
        if !same(&self.current, &state) {
            let previous = std::mem::replace(&mut self.current, state.clone());
            self.push_undo(previous);
        }
        self.current = state;
    }

    /// Records a change that was applied outside of `observe`, such as a reset.
    pub fn record_change(&mut self, before: T, after: T) {
        self.push_undo(before);
        self.current = after;
    }

    /// Replaces the observed state without recording a change, after a state was applied.
    pub fn set_current(&mut self, state: T) {
        self.current = state;
    }

    /// Returns the state to apply, or `None` when there is nothing to undo.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.has_unsaved_changes = true;
        Some(previous)
    }

    /// Returns the state to apply, or `None` when there is nothing to redo.
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.has_unsaved_changes = true;
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    fn push_undo(&mut self, state: T) {
        if self.undo.len() >= MAX_HISTORY {
            self.undo.remove(0);
        }
        self.undo.push(state);
        self.redo.clear();
        self.has_unsaved_changes = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(a: &i32, b: &i32) -> bool {
        a == b
    }

    #[test]
    fn unchanged_state_records_nothing() {
        let mut history = History::new(1);
        history.observe(1, eq);
        assert!(!history.can_undo());
        assert!(!history.has_unsaved_changes());
    }

    #[test]
    fn undo_and_redo_walk_the_changes() {
        let mut history = History::new(1);
        history.observe(2, eq);
        history.observe(3, eq);
        assert!(history.has_unsaved_changes());

        assert_eq!(history.undo(3), Some(2));
        history.set_current(2);
        assert_eq!(history.undo(2), Some(1));
        history.set_current(1);
        assert_eq!(history.undo(1), None);

        assert_eq!(history.redo(1), Some(2));
        history.set_current(2);
        history.observe(5, eq);
        assert!(!history.can_redo());
    }

    #[test]
    fn keeps_at_most_ten_steps() {
        let mut history = History::new(0);
        for value in 1..=15 {
            history.observe(value, eq);
        }
        let mut steps = 0;
        let mut current = 15;
        while let Some(previous) = history.undo(current) {
            current = previous;
            steps += 1;
        }
        assert_eq!(steps, MAX_HISTORY);
        assert_eq!(current, 5);
    }

    #[test]
    fn equality_function_can_ignore_fields() {
        let mut history = History::new((1, 'a'));
        history.observe((1, 'b'), |a, b| a.0 == b.0);
        assert!(!history.can_undo());
        assert!(!history.has_unsaved_changes());
    }

    #[test]
    fn saving_clears_the_unsaved_flag_only() {
        let mut history = History::new(1);
        history.observe(2, eq);
        history.mark_saved();
        assert!(!history.has_unsaved_changes());
        assert!(history.can_undo());
    }
}
