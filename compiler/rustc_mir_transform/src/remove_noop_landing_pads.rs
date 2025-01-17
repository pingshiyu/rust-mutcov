use crate::MirPass;
use rustc_index::bit_set::BitSet;
use rustc_middle::mir::patch::MirPatch;
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;
use rustc_target::spec::PanicStrategy;

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

/// A pass that removes noop landing pads and replaces jumps to them with
/// `None`. This is important because otherwise LLVM generates terrible
/// code for these.
pub struct RemoveNoopLandingPads;

impl<'tcx> MirPass<'tcx> for RemoveNoopLandingPads {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.panic_strategy() != PanicStrategy::Abort
    }

    fn run_pass(&self, _: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        debug!("remove_noop_landing_pads({:?})", body);
        self.remove_nop_landing_pads(body)
    }
}

impl RemoveNoopLandingPads {
    fn is_nop_landing_pad(
        &self,
        bb: BasicBlock,
        body: &Body<'_>,
        nop_landing_pads: &BitSet<BasicBlock>,
    ) -> bool {
        for stmt in &body[bb].statements {
            match &stmt.kind {
                StatementKind::FakeRead(..)
                | StatementKind::StorageLive(_)
                | StatementKind::StorageDead(_)
                | StatementKind::AscribeUserType(..)
                | StatementKind::Coverage(..)
                | StatementKind::Nop => {
                    // These are all noops in a landing pad
                }

                StatementKind::Assign(box (place, Rvalue::Use(_) | Rvalue::Discriminant(_))) => {
                    if mutate_condition!(place.as_local().is_some(), 283) {
                        // Writing to a local (e.g., a drop flag) does not
                        // turn a landing pad to a non-nop
                    } else {
                        return false;
                    }
                }

                StatementKind::Assign { .. }
                | StatementKind::SetDiscriminant { .. }
                | StatementKind::Deinit(..)
                | StatementKind::CopyNonOverlapping(..)
                | StatementKind::Retag { .. } => {
                    return false;
                }
            }
        }

        let terminator = body[bb].terminator();
        match terminator.kind {
            TerminatorKind::Goto { .. }
            | TerminatorKind::Resume
            | TerminatorKind::SwitchInt { .. }
            | TerminatorKind::FalseEdge { .. }
            | TerminatorKind::FalseUnwind { .. } => {
                terminator.successors().all(|&succ| nop_landing_pads.contains(succ))
            }
            TerminatorKind::GeneratorDrop
            | TerminatorKind::Yield { .. }
            | TerminatorKind::Return
            | TerminatorKind::Abort
            | TerminatorKind::Unreachable
            | TerminatorKind::Call { .. }
            | TerminatorKind::Assert { .. }
            | TerminatorKind::DropAndReplace { .. }
            | TerminatorKind::Drop { .. }
            | TerminatorKind::InlineAsm { .. } => false,
        }
    }

    fn remove_nop_landing_pads(&self, body: &mut Body<'_>) {
        // make sure there's a single resume block
        let resume_block = {
            let patch = MirPatch::new(body);
            let resume_block = patch.resume_block();
            patch.apply(body);
            resume_block
        };
        debug!("remove_noop_landing_pads: resume block is {:?}", resume_block);

        let mut jumps_folded = 0;
        let mut landing_pads_removed = 0;
        let mut nop_landing_pads = BitSet::new_empty(body.basic_blocks().len());

        // This is a post-order traversal, so that if A post-dominates B
        // then A will be visited before B.
        let postorder: Vec<_> = traversal::postorder(body).map(|(bb, _)| bb).collect();
        for bb in postorder {
            debug!("  processing {:?}", bb);
            if let Some(unwind) = body[bb].terminator_mut().unwind_mut() {
                if let Some(unwind_bb) = *unwind {
                    if mutate_condition!(nop_landing_pads.contains(unwind_bb), 284) {
                        debug!("    removing noop landing pad");
                        landing_pads_removed += 1;
                        *unwind = None;
                    }
                }
            }

            for target in body[bb].terminator_mut().successors_mut() {
                if mutate_condition!(*target != resume_block && nop_landing_pads.contains(*target), 285) {
                    debug!("    folding noop jump to {:?} to resume block", target);
                    *target = resume_block;
                    jumps_folded += 1;
                }
            }

            let is_nop_landing_pad = self.is_nop_landing_pad(bb, body, &nop_landing_pads);
            if mutate_condition!(is_nop_landing_pad, 286) {
                nop_landing_pads.insert(bb);
            }
            debug!("    is_nop_landing_pad({:?}) = {}", bb, is_nop_landing_pad);
        }

        debug!("removed {:?} jumps and {:?} landing pads", jumps_folded, landing_pads_removed);
    }
}
