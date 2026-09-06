# Constraints — THE LANGUAGE

## Primary constraint

The compiler avoids a conventional multi-pass AST-centered architecture, preferring a typed SSA/CPS-like representation directly from parsing.

## What it forces

Alternative intermediate representation design, and a compiler front-end that must justify every conventional structure it keeps.

## Research question

Which compiler abstractions are fundamental, and which are merely convenient engineering structures?

## What is explicitly out of scope

See the root [SCOPE.md](../../SCOPE.md) for the CORE / EXTENSION / EXPERIMENT
classification that applies to this repo.
