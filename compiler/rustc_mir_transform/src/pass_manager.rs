use std::borrow::Cow;

use rustc_middle::mir::{self, Body, MirPhase};
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;

use crate::{validate, MirPass};

macro_rules! mutate_condition{
    ($original_expression:expr, $mutation_number: literal) => {
        {
            if let Ok(env_mut_number) = std::env::var("RUSTC_MUTATION_NUMBER") {
                // println!("Found mutation number: {}, when potentially mutating {}", env_mut_number, $mutation_number);
                if $mutation_number == env_mut_number.parse::<i32>().unwrap() {
                    // println!("Mutation number matches, replacing expr with negate");
                    !$original_expression
                } else {
                    $original_expression
                }
            } else {
                println!("No env variable");
                $original_expression
            }
        }
    }
}

/// Just like `MirPass`, except it cannot mutate `Body`.
pub trait MirLint<'tcx> {
    fn name(&self) -> Cow<'_, str> {
        let name = std::any::type_name::<Self>();
        if let Some(tail) = name.rfind(':') {
            Cow::from(&name[tail + 1..])
        } else {
            Cow::from(name)
        }
    }

    fn is_enabled(&self, _sess: &Session) -> bool {
        true
    }

    fn run_lint(&self, tcx: TyCtxt<'tcx>, body: &Body<'tcx>);
}

/// An adapter for `MirLint`s that implements `MirPass`.
#[derive(Debug, Clone)]
pub struct Lint<T>(pub T);

impl<'tcx, T> MirPass<'tcx> for Lint<T>
where
    T: MirLint<'tcx>,
{
    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }

    fn is_enabled(&self, sess: &Session) -> bool {
        self.0.is_enabled(sess)
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        self.0.run_lint(tcx, body)
    }

    fn is_mir_dump_enabled(&self) -> bool {
        false
    }
}

pub struct WithMinOptLevel<T>(pub u32, pub T);

impl<'tcx, T> MirPass<'tcx> for WithMinOptLevel<T>
where
    T: MirPass<'tcx>,
{
    fn name(&self) -> Cow<'_, str> {
        self.1.name()
    }

    fn is_enabled(&self, sess: &Session) -> bool {
        sess.mir_opt_level() >= self.0 as usize
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        self.1.run_pass(tcx, body)
    }

    fn phase_change(&self) -> Option<MirPhase> {
        self.1.phase_change()
    }
}

pub fn run_passes<'tcx>(tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>, passes: &[&dyn MirPass<'tcx>]) {
    let start_phase = body.phase;
    let mut cnt = 0;

    let validate = tcx.sess.opts.debugging_opts.validate_mir;
    let overridden_passes = &tcx.sess.opts.debugging_opts.mir_enable_passes;
    trace!(?overridden_passes);

    if mutate_condition!(validate, 273) {
        validate_body(tcx, body, format!("start of phase transition from {:?}", start_phase));
    }

    for pass in passes {
        let name = pass.name();

        if let Some((_, polarity)) = overridden_passes.iter().rev().find(|(s, _)| s == &*name) {
            trace!(
                pass = %name,
                "{} as requested by flag",
                if mutate_condition!(*polarity, 274) { "Running" } else { "Not running" },
            );
            if mutate_condition!(!polarity, 275) {
                continue;
            }
        } else {
            if mutate_condition!(!pass.is_enabled(&tcx.sess), 276) {
                continue;
            }
        }
        let dump_enabled = pass.is_mir_dump_enabled();

        if mutate_condition!(dump_enabled, 277) {
            dump_mir(tcx, body, start_phase, &name, cnt, false);
        }

        pass.run_pass(tcx, body);

        if mutate_condition!(dump_enabled, 278) {
            dump_mir(tcx, body, start_phase, &name, cnt, true);
            cnt += 1;
        }

        if let Some(new_phase) = pass.phase_change() {
            if mutate_condition!(body.phase >= new_phase, 279) {
                panic!("Invalid MIR phase transition from {:?} to {:?}", body.phase, new_phase);
            }

            body.phase = new_phase;
        }

        if mutate_condition!(validate, 280) {
            validate_body(tcx, body, format!("after pass {}", pass.name()));
        }
    }

    if mutate_condition!(validate || body.phase == MirPhase::Optimized, 281) {
        validate_body(tcx, body, format!("end of phase transition to {:?}", body.phase));
    }
}

pub fn validate_body<'tcx>(tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>, when: String) {
    validate::Validator { when, mir_phase: body.phase }.run_pass(tcx, body);
}

pub fn dump_mir<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    phase: MirPhase,
    pass_name: &str,
    cnt: usize,
    is_after: bool,
) {
    let phase_index = phase as u32;

    mir::dump_mir(
        tcx,
        Some(&format_args!("{:03}-{:03}", phase_index, cnt)),
        pass_name,
        if mutate_condition!(is_after, 282) { &"after" } else { &"before" },
        body,
        |_, _| Ok(()),
    );
}
