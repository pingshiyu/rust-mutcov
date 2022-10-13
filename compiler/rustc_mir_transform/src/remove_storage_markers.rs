//! This pass removes storage markers if they won't be emitted during codegen.

use crate::MirPass;
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;

macro_rules! mutate_condition{
    ($original_expression:expr, $mutation_number: literal) => {
        {
            if let Ok(env_mut_number) = std::env::var("RUSTC_MUTATION_NUMBER") {
                
                if $mutation_number == env_mut_number.parse::<i32>().unwrap() {
                    
                    !$original_expression
                } else {
                    $original_expression
                }
            } else {
                
                $original_expression
            }
        }
    }
}

pub struct RemoveStorageMarkers;

impl<'tcx> MirPass<'tcx> for RemoveStorageMarkers {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.mir_opt_level() > 0
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        if mutate_condition!(tcx.sess.emit_lifetime_markers(), 287) {
            return;
        }

        trace!("Running RemoveStorageMarkers on {:?}", body.source);
        for data in body.basic_blocks_mut() {
            data.statements.retain(|statement| match statement.kind {
                StatementKind::StorageLive(..)
                | StatementKind::StorageDead(..)
                | StatementKind::Nop => false,
                _ => true,
            })
        }
    }
}
