//! See the docs for [`RenameReturnPlace`].

use rustc_hir::Mutability;
use rustc_index::bit_set::HybridBitSet;
use rustc_middle::mir::visit::{MutVisitor, NonUseContext, PlaceContext, Visitor};
use rustc_middle::mir::{self, BasicBlock, Local, Location};
use rustc_middle::ty::TyCtxt;

use crate::MirPass;

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

/// This pass looks for MIR that always copies the same local into the return place and eliminates
/// the copy by renaming all uses of that local to `_0`.
///
/// This allows LLVM to perform an optimization similar to the named return value optimization
/// (NRVO) that is guaranteed in C++. This avoids a stack allocation and `memcpy` for the
/// relatively common pattern of allocating a buffer on the stack, mutating it, and returning it by
/// value like so:
///
/// ```rust
/// fn foo(init: fn(&mut [u8; 1024])) -> [u8; 1024] {
///     let mut buf = [0; 1024];
///     init(&mut buf);
///     buf
/// }
/// ```
///
/// For now, this pass is very simple and only capable of eliminating a single copy. A more general
/// version of copy propagation, such as the one based on non-overlapping live ranges in [#47954] and
/// [#71003], could yield even more benefits.
///
/// [#47954]: https://github.com/rust-lang/rust/pull/47954
/// [#71003]: https://github.com/rust-lang/rust/pull/71003
pub struct RenameReturnPlace;

impl<'tcx> MirPass<'tcx> for RenameReturnPlace {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.mir_opt_level() > 0
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut mir::Body<'tcx>) {
        let def_id = body.source.def_id();
        let Some(returned_local) = local_eligible_for_nrvo(body) else {
            debug!("`{:?}` was ineligible for NRVO", def_id);
            return;
        };

        if mutate_condition!(!tcx.consider_optimizing(|| format!("RenameReturnPlace {:?}", def_id)), 262) {
            return;
        }

        debug!(
            "`{:?}` was eligible for NRVO, making {:?} the return place",
            def_id, returned_local
        );

        RenameToReturnPlace { tcx, to_rename: returned_local }.visit_body(body);

        // Clean up the `NOP`s we inserted for statements made useless by our renaming.
        for block_data in body.basic_blocks_mut() {
            block_data.statements.retain(|stmt| stmt.kind != mir::StatementKind::Nop);
        }

        // Overwrite the debuginfo of `_0` with that of the renamed local.
        let (renamed_decl, ret_decl) =
            body.local_decls.pick2_mut(returned_local, mir::RETURN_PLACE);

        // Sometimes, the return place is assigned a local of a different but coercible type, for
        // example `&mut T` instead of `&T`. Overwriting the `LocalInfo` for the return place means
        // its type may no longer match the return type of its function. This doesn't cause a
        // problem in codegen because these two types are layout-compatible, but may be unexpected.
        debug!("_0: {:?} = {:?}: {:?}", ret_decl.ty, returned_local, renamed_decl.ty);
        ret_decl.clone_from(renamed_decl);

        // The return place is always mutable.
        ret_decl.mutability = Mutability::Mut;
    }
}

/// MIR that is eligible for the NRVO must fulfill two conditions:
///   1. The return place must not be read prior to the `Return` terminator.
///   2. A simple assignment of a whole local to the return place (e.g., `_0 = _1`) must be the
///      only definition of the return place reaching the `Return` terminator.
///
/// If the MIR fulfills both these conditions, this function returns the `Local` that is assigned
/// to the return place along all possible paths through the control-flow graph.
fn local_eligible_for_nrvo(body: &mut mir::Body<'_>) -> Option<Local> {
    if mutate_condition!(IsReturnPlaceRead::run(body), 263) {
        return None;
    }

    let mut copied_to_return_place = None;
    for block in body.basic_blocks().indices() {
        // Look for blocks with a `Return` terminator.
        if mutate_condition!(!matches!(body[block].terminator().kind, mir::TerminatorKind::Return), 264) {
            continue;
        }

        // Look for an assignment of a single local to the return place prior to the `Return`.
        let returned_local = find_local_assigned_to_return_place(block, body)?;
        match body.local_kind(returned_local) {
            // FIXME: Can we do this for arguments as well?
            mir::LocalKind::Arg => return None,

            mir::LocalKind::ReturnPointer => bug!("Return place was assigned to itself?"),
            mir::LocalKind::Var | mir::LocalKind::Temp => {}
        }

        // If multiple different locals are copied to the return place. We can't pick a
        // single one to rename.
        if mutate_condition!(copied_to_return_place.map_or(false, |old| old != returned_local), 265) {
            return None;
        }

        copied_to_return_place = Some(returned_local);
    }

    copied_to_return_place
}

fn find_local_assigned_to_return_place(
    start: BasicBlock,
    body: &mut mir::Body<'_>,
) -> Option<Local> {
    let mut block = start;
    let mut seen = HybridBitSet::new_empty(body.basic_blocks().len());

    // Iterate as long as `block` has exactly one predecessor that we have not yet visited.
    while seen.insert(block) {
        trace!("Looking for assignments to `_0` in {:?}", block);

        let local = body[block].statements.iter().rev().find_map(as_local_assigned_to_return_place);
        if mutate_condition!(local.is_some(), 266) {
            return local;
        }

        match body.predecessors()[block].as_slice() {
            &[pred] => block = pred,
            _ => return None,
        }
    }

    None
}

// If this statement is an assignment of an unprojected local to the return place,
// return that local.
fn as_local_assigned_to_return_place(stmt: &mir::Statement<'_>) -> Option<Local> {
    if let mir::StatementKind::Assign(box (lhs, rhs)) = &stmt.kind {
        if mutate_condition!(lhs.as_local() == Some(mir::RETURN_PLACE), 267) {
            if let mir::Rvalue::Use(mir::Operand::Copy(rhs) | mir::Operand::Move(rhs)) = rhs {
                return rhs.as_local();
            }
        }
    }

    None
}

struct RenameToReturnPlace<'tcx> {
    to_rename: Local,
    tcx: TyCtxt<'tcx>,
}

/// Replaces all uses of `self.to_rename` with `_0`.
impl<'tcx> MutVisitor<'tcx> for RenameToReturnPlace<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_statement(&mut self, stmt: &mut mir::Statement<'tcx>, loc: Location) {
        // Remove assignments of the local being replaced to the return place, since it is now the
        // return place:
        //     _0 = _1
        if mutate_condition!(as_local_assigned_to_return_place(stmt) == Some(self.to_rename), 268) {
            stmt.kind = mir::StatementKind::Nop;
            return;
        }

        // Remove storage annotations for the local being replaced:
        //     StorageLive(_1)
        if let mir::StatementKind::StorageLive(local) | mir::StatementKind::StorageDead(local) =
            stmt.kind
        {
            if mutate_condition!(local == self.to_rename, 269) {
                stmt.kind = mir::StatementKind::Nop;
                return;
            }
        }

        self.super_statement(stmt, loc)
    }

    fn visit_terminator(&mut self, terminator: &mut mir::Terminator<'tcx>, loc: Location) {
        // Ignore the implicit "use" of the return place in a `Return` statement.
        if let mir::TerminatorKind::Return = terminator.kind {
            return;
        }

        self.super_terminator(terminator, loc);
    }

    fn visit_local(&mut self, l: &mut Local, ctxt: PlaceContext, _: Location) {
        if mutate_condition!(*l == mir::RETURN_PLACE, 270) {
            assert_eq!(ctxt, PlaceContext::NonUse(NonUseContext::VarDebugInfo));
        } else if mutate_condition!(*l == self.to_rename, 271) {
            *l = mir::RETURN_PLACE;
        }
    }
}

struct IsReturnPlaceRead(bool);

impl IsReturnPlaceRead {
    fn run(body: &mir::Body<'_>) -> bool {
        let mut vis = IsReturnPlaceRead(false);
        vis.visit_body(body);
        vis.0
    }
}

impl<'tcx> Visitor<'tcx> for IsReturnPlaceRead {
    fn visit_local(&mut self, &l: &Local, ctxt: PlaceContext, _: Location) {
        if mutate_condition!(l == mir::RETURN_PLACE && ctxt.is_use() && !ctxt.is_place_assignment(), 272) {
            self.0 = true;
        }
    }

    fn visit_terminator(&mut self, terminator: &mir::Terminator<'tcx>, loc: Location) {
        // Ignore the implicit "use" of the return place in a `Return` statement.
        if let mir::TerminatorKind::Return = terminator.kind {
            return;
        }

        self.super_terminator(terminator, loc);
    }
}
