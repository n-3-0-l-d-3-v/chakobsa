//! Compile time vs. program size — measured, not assumed, per this
//! project's Definition of Done. Two separate questions: how parsing
//! (straight to typed SSA, ticket 003) scales, and how much codegen
//! (register allocation + lowering, ticket 004) adds on top for the
//! same program.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// `fn f() -> i64 { let v0 = 0; v0 = v0 + 1; v0 = v0 + 1; ... return v0; }`
/// — `n` sequential reassignments of one variable, the simplest possible
/// straight-line program shape that scales linearly in size.
fn straight_line_program(n: usize) -> String {
    let mut src = String::from("fn f() -> i64 { let v = 0; ");
    for _ in 0..n {
        src.push_str("v = v + 1; ");
    }
    src.push_str("return v; }");
    src
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_parse");
    for size in [10usize, 100, 1_000, 10_000] {
        let src = straight_line_program(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &src, |b, src| {
            b.iter(|| black_box(parser::parse(src).unwrap()));
        });
    }
    group.finish();
}

/// Deliberately smaller upper bound than `bench_parse`'s: register
/// allocation's interference graph is built with a naive O(block size²)
/// pairwise loop (`regalloc.rs`), a documented trade-off (see
/// `liveness.rs`'s module doc and
/// `docs/design/decisions/ADR-006-differential-testing-and-benchmarks.md`)
/// that becomes measured, quadratic reality once *every* value in a
/// program lives in one giant block, as this straight-line generator
/// produces. 10,000 took multiple seconds *per iteration* during this
/// benchmark's own development — informative to have measured once, not
/// useful to force every future `cargo bench` run to pay for.
fn bench_codegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_codegen");
    for size in [10usize, 100, 500, 1_000] {
        let module = parser::parse(&straight_line_program(size)).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(size), &module, |b, module| {
            b.iter(|| black_box(codegen::compile_module(module).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse, bench_codegen);
criterion_main!(benches);
