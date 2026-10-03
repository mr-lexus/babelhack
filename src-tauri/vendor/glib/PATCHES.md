# Local security backport

Source: crates.io glib 0.18.5, gtk-rs Project Developers, MIT (LICENSE and COPYRIGHT retained).

Only functional change: src/variant_iter.rs, VariantStrIter::impl_get, makes the output pointer mutable and passes &mut p to g_variant_get_child.

Upstream fix: https://github.com/gtk-rs/gtk-rs-core/pull/1343
Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html

This backport preserves the 0.18 API required by GTK3/Tauri. Remove the path override when that dependency chain moves to a fixed compatible upstream release. Do not mistake the unchanged version number for an unmodified crates.io copy.

Regression: platform::tests::glib_string_iterator_backport on Linux, including an optimized test profile. The rest of the crate is unmodified published source; packaging metadata and original licenses are preserved.
