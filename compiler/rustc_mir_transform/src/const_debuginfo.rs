//! Finds locals which are assigned once to a const and unused except for debuginfo and converts
//! their debuginfo to use the const directly, allowing the local to be removed.

use rustc_middle::{
    mir::{
        visit::{PlaceContext, Visitor},
        Body, Constant, Local, Location, Operand, Rvalue, StatementKind, VarDebugInfoContents,
    },
    ty::TyCtxt,
};

use crate::MirPass;
use rustc_index::{bit_set::BitSet, vec::IndexVec};

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

pub struct ConstDebugInfo;

impl<'tcx> MirPass<'tcx> for ConstDebugInfo {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.opts.debugging_opts.unsound_mir_opts && sess.mir_opt_level() > 0
    }

    fn run_pass(&self, _: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        trace!("running ConstDebugInfo on {:?}", body.source);

        for (local, constant) in find_optimization_oportunities(body) {
            for debuginfo in &mut body.var_debug_info {
                if let VarDebugInfoContents::Place(p) = debuginfo.value {
                    if mutate_condition!(p.local == local && p.projection.is_empty(), 32) {
                        trace!(
                            "changing debug info for {:?} from place {:?} to constant {:?}",
                            debuginfo.name,
                            p,
                            constant
                        );
                        debuginfo.value = VarDebugInfoContents::Const(constant);
                    }
                }
            }
        }
    }
}

struct LocalUseVisitor {
    local_mutating_uses: IndexVec<Local, u8>,
    local_assignment_locations: IndexVec<Local, Option<Location>>,
}

fn find_optimization_oportunities<'tcx>(body: &Body<'tcx>) -> Vec<(Local, Constant<'tcx>)> {
    let mut visitor = LocalUseVisitor {
        local_mutating_uses: IndexVec::from_elem(0, &body.local_decls),
        local_assignment_locations: IndexVec::from_elem(None, &body.local_decls),
    };

    visitor.visit_body(body);

    let mut locals_to_debuginfo = BitSet::new_empty(body.local_decls.len());
    for debuginfo in &body.var_debug_info {
        if let VarDebugInfoContents::Place(p) = debuginfo.value && let Some(l) = p.as_local() {
            locals_to_debuginfo.insert(l);
        }
    }

    let mut eligible_locals = Vec::new();
    for (local, mutating_uses) in visitor.local_mutating_uses.drain_enumerated(..) {
        if mutate_condition!(mutating_uses != 1 || !locals_to_debuginfo.contains(local), 33) {
            continue;
        }

        if let Some(location) = visitor.local_assignment_locations[local] {
            let bb = &body[location.block];

            // The value is assigned as the result of a call, not a constant
            if mutate_condition!(bb.statements.len() == location.statement_index, 34) {
                continue;
            }

            if let StatementKind::Assign(box (p, Rvalue::Use(Operand::Constant(box c)))) =
                &bb.statements[location.statement_index].kind
            {
                if let Some(local) = p.as_local() {
                    eligible_locals.push((local, *c));
                }
            }
        }
    }

    eligible_locals
}

impl Visitor<'_> for LocalUseVisitor {
    fn visit_local(&mut self, local: &Local, context: PlaceContext, location: Location) {
        if mutate_condition!(context.is_mutating_use(), 35) {
            self.local_mutating_uses[*local] = self.local_mutating_uses[*local].saturating_add(1);

            if mutate_condition!(context.is_place_assignment(), 36) {
                self.local_assignment_locations[*local] = Some(location);
            }
        }
    }
}
