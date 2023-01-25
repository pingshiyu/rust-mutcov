//! This pass replaces a drop of a type that does not need dropping, with a goto.
//!
//! When the MIR is built, we check `needs_drop` before emitting a `Drop` for a place. This pass is
//! useful because (unlike MIR building) it runs after type checking, so it can make use of
//! `Reveal::All` to provide more precise type information.

use crate::MirPass;
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;

use super::simplify::simplify_cfg;

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

pub struct RemoveUnneededDrops;

impl<'tcx> MirPass<'tcx> for RemoveUnneededDrops {
    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        trace!("Running RemoveUnneededDrops on {:?}", body.source);

        let did = body.source.def_id();
        let param_env = tcx.param_env_reveal_all_normalized(did);
        let mut should_simplify = false;

        let (basic_blocks, local_decls) = body.basic_blocks_and_local_decls_mut();
        for block in basic_blocks {
            let terminator = block.terminator_mut();
            if let TerminatorKind::Drop { place, target, .. } = terminator.kind {
                let ty = place.ty(local_decls, tcx);
                if mutate_condition!(ty.ty.needs_drop(tcx, param_env), 292) {
                    continue;
                }
                if mutate_condition!(!tcx.consider_optimizing(|| format!("RemoveUnneededDrops {:?} ", did)), 293) {
                    continue;
                }
                debug!("SUCCESS: replacing `drop` with goto({:?})", target);
                terminator.kind = TerminatorKind::Goto { target };
                should_simplify = true;
            }
        }

        // if we applied optimizations, we potentially have some cfg to cleanup to
        // make it easier for further passes
        if mutate_condition!(should_simplify, 294) {
            simplify_cfg(tcx, body);
        }
    }
}
