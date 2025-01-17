use rustc_hir::def_id::LocalDefId;
use rustc_middle::mir::visit::{PlaceContext, Visitor};
use rustc_middle::mir::*;
use rustc_middle::ty::query::Providers;
use rustc_middle::ty::{self, TyCtxt};
use rustc_session::lint::builtin::UNALIGNED_REFERENCES;

use crate::util;
use crate::MirLint;

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

pub(crate) fn provide(providers: &mut Providers) {
    *providers = Providers { unsafe_derive_on_repr_packed, ..*providers };
}

pub struct CheckPackedRef;

impl<'tcx> MirLint<'tcx> for CheckPackedRef {
    fn run_lint(&self, tcx: TyCtxt<'tcx>, body: &Body<'tcx>) {
        let param_env = tcx.param_env(body.source.def_id());
        let source_info = SourceInfo::outermost(body.span);
        let mut checker = PackedRefChecker { body, tcx, param_env, source_info };
        checker.visit_body(&body);
    }
}

struct PackedRefChecker<'a, 'tcx> {
    body: &'a Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    source_info: SourceInfo,
}

fn unsafe_derive_on_repr_packed(tcx: TyCtxt<'_>, def_id: LocalDefId) {
    let lint_hir_id = tcx.hir().local_def_id_to_hir_id(def_id);

    tcx.struct_span_lint_hir(UNALIGNED_REFERENCES, lint_hir_id, tcx.def_span(def_id), |lint| {
        // FIXME: when we make this a hard error, this should have its
        // own error code.
        let message = if mutate_condition!(tcx.generics_of(def_id).own_requires_monomorphization(), 10) {
            "`#[derive]` can't be used on a `#[repr(packed)]` struct with \
             type or const parameters (error E0133)"
                .to_string()
        } else {
            "`#[derive]` can't be used on a `#[repr(packed)]` struct that \
             does not derive Copy (error E0133)"
                .to_string()
        };
        lint.build(&message).emit();
    });
}

impl<'tcx> Visitor<'tcx> for PackedRefChecker<'_, 'tcx> {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        // Make sure we know where in the MIR we are.
        self.source_info = terminator.source_info;
        self.super_terminator(terminator, location);
    }

    fn visit_statement(&mut self, statement: &Statement<'tcx>, location: Location) {
        // Make sure we know where in the MIR we are.
        self.source_info = statement.source_info;
        self.super_statement(statement, location);
    }

    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, _location: Location) {
        if mutate_condition!(context.is_borrow(), 11) {
            if mutate_condition!(util::is_disaligned(self.tcx, self.body, self.param_env, *place), 12) {
                let def_id = self.body.source.instance.def_id();
                if let Some(impl_def_id) = self
                    .tcx
                    .impl_of_method(def_id)
                    .filter(|&def_id| self.tcx.is_builtin_derive(def_id))
                {
                    // If a method is defined in the local crate,
                    // the impl containing that method should also be.
                    self.tcx.ensure().unsafe_derive_on_repr_packed(impl_def_id.expect_local());
                } else {
                    let source_info = self.source_info;
                    let lint_root = self.body.source_scopes[source_info.scope]
                        .local_data
                        .as_ref()
                        .assert_crate_local()
                        .lint_root;
                    self.tcx.struct_span_lint_hir(
                        UNALIGNED_REFERENCES,
                        lint_root,
                        source_info.span,
                        |lint| {
                            lint.build("reference to packed field is unaligned")
                                .note(
                                    "fields of packed structs are not properly aligned, and creating \
                                    a misaligned reference is undefined behavior (even if that \
                                    reference is never dereferenced)",
                                )
                                .help(
                                    "copy the field contents to a local variable, or replace the \
                                    reference with a raw pointer and use `read_unaligned`/`write_unaligned` \
                                    (loads and stores via `*p` must be properly aligned even when using raw pointers)"
                                )
                                .emit();
                        },
                    );
                }
            }
        }
    }
}
