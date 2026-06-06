# TODO

Deferred items from the M1 final review (2026-06-06). None block M2.

## Design question

- Duplicate-marker detection uses exact f64 equality. Near-duplicates (e.g. `10.0` vs `10.001`) pass validation and produce a ~1ms overlapping song instead of an error. Decide whether `plan()` should reject (or warn about) markers closer than some epsilon, and what the threshold should be.

## Next milestone

- M2 (egui GUI) needs its own implementation plan written against the real core API before any GUI work starts (see CLAUDE.md).
