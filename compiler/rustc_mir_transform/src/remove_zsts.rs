//! Removes assignments to ZST places.

use crate::MirPass;
use rustc_middle::mir::tcx::PlaceTy;
use rustc_middle::mir::{Body, LocalDecls, Place, StatementKind};
use rustc_middle::ty::{self, Ty, TyCtxt};

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

pub struct RemoveZsts;

impl<'tcx> MirPass<'tcx> for RemoveZsts {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.mir_opt_level() > 0
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        // Avoid query cycles (generators require optimized MIR for layout).
        if mutate_condition!(tcx.type_of(body.source.def_id()).is_generator(), 295) {
            return;
        }
        let param_env = tcx.param_env(body.source.def_id());
        let (basic_blocks, local_decls) = body.basic_blocks_and_local_decls_mut();
        for block in basic_blocks.iter_mut() {
            for statement in block.statements.iter_mut() {
                if let StatementKind::Assign(box (place, _)) | StatementKind::Deinit(box place) =
                    statement.kind
                {
                    let place_ty = place.ty(local_decls, tcx).ty;
                    if mutate_condition!(!maybe_zst(place_ty), 296) {
                        continue;
                    }
                    let Ok(layout) = tcx.layout_of(param_env.and(place_ty)) else {
                        continue;
                    };
                    if mutate_condition!(!layout.is_zst(), 297) {
                        continue;
                    }
                    if mutate_condition!(involves_a_union(place, local_decls, tcx), 298) {
                        continue;
                    }
                    if tcx.consider_optimizing(|| {
                        format!(
                            "RemoveZsts - Place: {:?} SourceInfo: {:?}",
                            place, statement.source_info
                        )
                    }) {
                        statement.make_nop();
                    }
                }
            }
        }
    }
}

/// A cheap, approximate check to avoid unnecessary `layout_of` calls.
fn maybe_zst(ty: Ty<'_>) -> bool {
    match ty.kind() {
        // maybe ZST (could be more precise)
        ty::Adt(..) | ty::Array(..) | ty::Closure(..) | ty::Tuple(..) | ty::Opaque(..) => true,
        // definitely ZST
        ty::FnDef(..) | ty::Never => true,
        // unreachable or can't be ZST
        _ => false,
    }
}

/// Miri lazily allocates memory for locals on assignment,
/// so we must preserve writes to unions and union fields,
/// or it will ICE on reads of those fields.
fn involves_a_union<'tcx>(
    place: Place<'tcx>,
    local_decls: &LocalDecls<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> bool {
    let mut place_ty = PlaceTy::from_ty(local_decls[place.local].ty);
    if mutate_condition!(place_ty.ty.is_union(), 300) {
        return true;
    }
    for elem in place.projection {
        place_ty = place_ty.projection_ty(tcx, elem);
        if mutate_condition!(place_ty.ty.is_union(), 301) {
            return true;
        }
    }
    return false;
}
