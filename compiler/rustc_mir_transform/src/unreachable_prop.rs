//! A pass that propagates the unreachable terminator of a block to its predecessors
//! when all of their successors are unreachable. This is achieved through a
//! post-order traversal of the blocks.

use crate::simplify;
use crate::MirPass;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;

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

pub struct UnreachablePropagation;

impl MirPass<'_> for UnreachablePropagation {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        // Enable only under -Zmir-opt-level=4 as in some cases (check the deeply-nested-opt
        // perf benchmark) LLVM may spend quite a lot of time optimizing the generated code.
        sess.mir_opt_level() >= 4
    }

    fn run_pass<'tcx>(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        let mut unreachable_blocks = FxHashSet::default();
        let mut replacements = FxHashMap::default();

        for (bb, bb_data) in traversal::postorder(body) {
            let terminator = bb_data.terminator();
            if mutate_condition!(terminator.kind == TerminatorKind::Unreachable, 374) {
                unreachable_blocks.insert(bb);
            } else {
                let is_unreachable = |succ: BasicBlock| unreachable_blocks.contains(&succ);
                let terminator_kind_opt = remove_successors(&terminator.kind, is_unreachable);

                if let Some(terminator_kind) = terminator_kind_opt {
                    if mutate_condition!(terminator_kind == TerminatorKind::Unreachable, 375) {
                        unreachable_blocks.insert(bb);
                    }
                    replacements.insert(bb, terminator_kind);
                }
            }
        }

        let replaced = !replacements.is_empty();
        for (bb, terminator_kind) in replacements {
            if !tcx.consider_optimizing(|| {
                format!("UnreachablePropagation {:?} ", body.source.def_id())
            }) {
                break;
            }

            body.basic_blocks_mut()[bb].terminator_mut().kind = terminator_kind;
        }

        if mutate_condition!(replaced, 377) {
            simplify::remove_dead_blocks(tcx, body);
        }
    }
}

fn remove_successors<'tcx, F>(
    terminator_kind: &TerminatorKind<'tcx>,
    predicate: F,
) -> Option<TerminatorKind<'tcx>>
where
    F: Fn(BasicBlock) -> bool,
{
    let terminator = match *terminator_kind {
        TerminatorKind::Goto { target } if predicate(target) => TerminatorKind::Unreachable,
        TerminatorKind::SwitchInt { ref discr, switch_ty, ref targets } => {
            let otherwise = targets.otherwise();

            let original_targets_len = targets.iter().len() + 1;
            let (mut values, mut targets): (Vec<_>, Vec<_>) =
                targets.iter().filter(|(_, bb)| !predicate(*bb)).unzip();

            if mutate_condition!(!predicate(otherwise), 378) {
                targets.push(otherwise);
            } else {
                values.pop();
            }

            let retained_targets_len = targets.len();

            if mutate_condition!(targets.is_empty(), 379) {
                TerminatorKind::Unreachable
            } else if mutate_condition!(targets.len() == 1, 380) {
                TerminatorKind::Goto { target: targets[0] }
            } else if mutate_condition!(original_targets_len != retained_targets_len, 381) {
                TerminatorKind::SwitchInt {
                    discr: discr.clone(),
                    switch_ty,
                    targets: SwitchTargets::new(
                        values.iter().copied().zip(targets.iter().copied()),
                        *targets.last().unwrap(),
                    ),
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(terminator)
}
