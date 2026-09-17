//! Persistent low-level timeline expressions and binding.

use std::fmt;
use std::ops::{Add, Shl};
use std::rc::Rc;

use crate::runtime::RuntimeResult;
use rsvz_backend_api::{BoardReadinessBackend, GameUiBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::model::{RelativeTime, Wave};

use super::input::WaveSet;

pub(super) type RuntimeCallback = Box<dyn FnMut() -> RuntimeResult<()> + 'static>;
type CommitAction = Box<dyn FnOnce() + 'static>;

pub(super) struct PreparedOp {
    pub(super) time: RelativeTime,
    pub(super) callback: RuntimeCallback,
}

pub(super) struct PrepareContext<'a> {
    wave: i32,
    semantic_time: i128,
    entries: &'a mut Vec<PreparedOp>,
    commit_actions: &'a mut Vec<CommitAction>,
    errors: &'a mut Vec<String>,
}

impl PrepareContext<'_> {
    #[must_use]
    pub(super) const fn wave(&self) -> i32 {
        self.wave
    }

    #[must_use]
    pub(super) const fn semantic_time(&self) -> i128 {
        self.semantic_time
    }

    pub(super) fn push(&mut self, time: i128, callback: RuntimeCallback) {
        match i32::try_from(time) {
            Ok(time) => self.entries.push(PreparedOp {
                time: RelativeTime::new(Wave(self.wave), time),
                callback,
            }),
            Err(_) => self.error(format!(
                "wave {} timeline time {time} is outside the i32 range",
                self.wave
            )),
        }
    }

    pub(super) fn error(&mut self, error: impl fmt::Display) {
        self.errors.push(error.to_string());
    }

    pub(super) fn on_commit(&mut self, action: impl FnOnce() + 'static) {
        self.commit_actions.push(Box::new(action));
    }
}

pub(super) trait Leaf: 'static {
    fn prepare(&self, context: &mut PrepareContext<'_>);
}

enum Node {
    Empty,
    Leaf(Rc<dyn Leaf>),
    Concat {
        left: Expr,
        right: Expr,
        right_offset: i128,
    },
    Error(Box<str>),
}

/// Persistent low-level expression. Factories check the required current-backend
/// capabilities before erasing their prepared leaves.
pub struct Expr {
    root: Rc<Node>,
    span: i128,
}

/// Alias for the current low-level expression type.
pub type LowExpr = Expr;

impl Clone for Expr {
    fn clone(&self) -> Self {
        Self {
            root: Rc::clone(&self.root),
            span: self.span,
        }
    }
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expr").field("span", &self.span).finish_non_exhaustive()
    }
}

impl Expr {
    pub(super) fn empty() -> Self {
        Self {
            root: Rc::new(Node::Empty),
            span: 0,
        }
    }

    pub(super) fn from_leaf(leaf: impl Leaf) -> Self {
        Self {
            root: Rc::new(Node::Leaf(Rc::new(leaf))),
            span: 0,
        }
    }

    pub(super) fn error(error: impl Into<Box<str>>) -> Self {
        Self {
            root: Rc::new(Node::Error(error.into())),
            span: 0,
        }
    }

    pub(super) fn delay(frames: i32) -> Self {
        Self {
            root: Rc::new(Node::Empty),
            span: i128::from(frames),
        }
    }

    fn concat(self, right: Self) -> Self {
        let right_offset = self.span;
        let Some(span) = self.span.checked_add(right.span) else {
            return Self::error("low-level expression cursor overflowed i128");
        };
        Self {
            root: Rc::new(Node::Concat {
                left: self,
                right,
                right_offset,
            }),
            span,
        }
    }

    fn bind(&self, waves: WaveSet, base_time: i32)
    where
        CurrentBackend: GameUiBackend + BoardReadinessBackend,
    {
        let mut entries = Vec::new();
        let mut commit_actions = Vec::new();
        let mut errors = Vec::new();
        self.prepare_node(
            &self.root,
            waves,
            i128::from(base_time),
            0,
            &mut entries,
            &mut commit_actions,
            &mut errors,
        );

        if !errors.is_empty() {
            for error in errors {
                crate::registration::record_error(error);
            }
            return;
        }

        let entries = entries.into_iter().map(|entry| (entry.time, entry.callback)).collect();
        let registrations = rsvz_schedule::timeline::with_timeline(|timeline| timeline.at_times_runtime(entries));
        match registrations {
            Ok(registrations) => {
                for action in commit_actions {
                    action();
                }
                rsvz_game::timeline::consume_registrations(registrations);
            }
            Err(error) => crate::registration::record_error(error),
        }
    }

    fn prepare_node(
        &self, node: &Node, waves: WaveSet, base_time: i128, offset: i128, entries: &mut Vec<PreparedOp>,
        commit_actions: &mut Vec<CommitAction>, errors: &mut Vec<String>,
    ) {
        match node {
            Node::Empty => {}
            Node::Error(error) => errors.push(error.to_string()),
            Node::Leaf(leaf) => {
                let Some(semantic_time) = base_time.checked_add(offset) else {
                    errors.push("low-level expression timeline offset overflowed i128".to_owned());
                    return;
                };
                for wave in waves.iter() {
                    leaf.prepare(&mut PrepareContext {
                        wave,
                        semantic_time,
                        entries,
                        commit_actions,
                        errors,
                    });
                }
            }
            Node::Concat {
                left,
                right,
                right_offset,
            } => {
                self.prepare_node(&left.root, waves, base_time, offset, entries, commit_actions, errors);
                let Some(offset) = offset.checked_add(*right_offset) else {
                    errors.push("low-level expression timeline offset overflowed i128".to_owned());
                    return;
                };
                self.prepare_node(&right.root, waves, base_time, offset, entries, commit_actions, errors);
            }
        }
    }

    fn record_unbound_errors(&self, base_time: i32) {
        let mut entries = Vec::new();
        let mut commit_actions = Vec::new();
        let mut errors = Vec::new();
        self.prepare_node(
            &self.root,
            WaveSet::from_bits(1),
            i128::from(base_time),
            0,
            &mut entries,
            &mut commit_actions,
            &mut errors,
        );
        for error in errors {
            crate::registration::record_error(error);
        }
    }
}

impl Add for Expr {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.concat(rhs)
    }
}

impl Add<&Self> for Expr {
    type Output = Self;

    fn add(self, rhs: &Self) -> Self::Output {
        self.concat(rhs.clone())
    }
}

impl Shl<Expr> for i32
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    type Output = ();

    fn shl(self, rhs: Expr) -> Self::Output {
        bind_current(&rhs, self);
    }
}

impl Shl<&Expr> for i32
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    type Output = ();

    fn shl(self, rhs: &Expr) -> Self::Output {
        bind_current(rhs, self);
    }
}

impl Shl<Expr> for (i32, i32)
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    type Output = ();

    fn shl(self, rhs: Expr) -> Self::Output {
        bind_explicit(&rhs, self.0, self.1);
    }
}

impl Shl<&Expr> for (i32, i32)
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    type Output = ();

    fn shl(self, rhs: &Expr) -> Self::Output {
        bind_explicit(rhs, self.0, self.1);
    }
}

fn bind_current(expr: &Expr, time: i32)
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    let Some(bits) = crate::registration::require_current_wave_bits() else {
        expr.record_unbound_errors(time);
        return;
    };
    expr.bind(WaveSet::from_bits(bits), time);
}

fn bind_explicit(expr: &Expr, wave: i32, time: i32)
where
    CurrentBackend: GameUiBackend + BoardReadinessBackend,
{
    match WaveSet::single(wave) {
        Ok(waves) => expr.bind(waves, time),
        Err(error) => {
            crate::registration::record_error(error);
            expr.record_unbound_errors(time);
        }
    }
}

/// Returns the additive identity for low-level expressions.
#[must_use]
pub fn empty() -> LowExpr {
    Expr::empty()
}

#[cfg(all(test, feature = "pvz-emulator"))]
mod tests {
    use super::*;
    use crate::dsl::wave;
    use std::cell::RefCell;
    struct ProbeLeaf(Rc<RefCell<Vec<(i32, i128)>>>);

    impl Leaf for ProbeLeaf {
        fn prepare(&self, context: &mut PrepareContext<'_>) {
            self.0.borrow_mut().push((context.wave(), context.semantic_time()));
            context.push(context.semantic_time(), Box::new(|| Ok(())));
        }
    }

    struct InvalidLeaf;

    impl Leaf for InvalidLeaf {
        fn prepare(&self, context: &mut PrepareContext<'_>) {
            context.error("leaf validation failed");
        }
    }

    fn probe(log: &Rc<RefCell<Vec<(i32, i128)>>>) -> Expr {
        Expr::from_leaf(ProbeLeaf(Rc::clone(log)))
    }

    fn reset() {
        rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear_all());
    }

    #[test]
    fn reusable_expression_keeps_its_cursor_span() {
        reset();
        let log = Rc::new(RefCell::new(Vec::new()));
        let reusable = probe(&log) + Expr::delay(107) + probe(&log);

        crate::registration::run_script(|| -> RuntimeResult<()> {
            wave(1);
            359 << (probe(&log) + &reusable + probe(&log));
            500 << &reusable;
            Ok(())
        })
        .expect("register");

        assert_eq!(
            *log.borrow(),
            [(1, 359), (1, 359), (1, 466), (1, 466), (1, 500), (1, 607)]
        );
    }

    #[test]
    fn expression_errors_prevent_the_whole_batch_from_registering() {
        reset();
        let log = Rc::new(RefCell::new(Vec::new()));
        let result = crate::registration::run_script(|| -> RuntimeResult<()> {
            wave(1);
            100 << (probe(&log) + Expr::error("bad leaf") + probe(&log));
            Ok(())
        });

        assert_eq!(result.expect_err("invalid expression").message().as_ref(), "bad leaf");
        assert_eq!(
            rsvz_schedule::timeline::with_timeline(|timeline| timeline.diagnostics().pending_count),
            0
        );
    }

    #[test]
    fn missing_or_invalid_wave_does_not_hide_embedded_expression_errors() {
        reset();
        let missing = crate::registration::run_script(|| -> RuntimeResult<()> {
            100 << (Expr::error("bad expression") + Expr::from_leaf(InvalidLeaf));
            Ok(())
        })
        .expect_err("missing wave and expression error");
        assert_eq!(
            missing.message().as_ref(),
            "timed DSL operation requires wave(...) or waves(...) first; bad expression; leaf validation failed"
        );

        let invalid = crate::registration::run_script(|| -> RuntimeResult<()> {
            (0, 100) << (Expr::error("also bad") + Expr::from_leaf(InvalidLeaf));
            Ok(())
        })
        .expect_err("invalid explicit wave and expression error");
        assert_eq!(
            invalid.message().as_ref(),
            "wave must be in 1..=20, got 0; also bad; leaf validation failed"
        );
    }

    #[test]
    fn empty_is_a_no_op_and_additive_identity() {
        reset();
        let log = Rc::new(RefCell::new(Vec::new()));
        crate::registration::run_script(|| -> RuntimeResult<()> {
            wave(2);
            10 << Expr::empty();
            20 << (Expr::empty() + probe(&log));
            30 << (probe(&log) + Expr::empty());
            Ok(())
        })
        .expect("register");

        assert_eq!(*log.borrow(), [(2, 20), (2, 30)]);
    }
}
