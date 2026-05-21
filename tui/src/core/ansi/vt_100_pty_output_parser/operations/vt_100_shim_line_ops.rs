// Copyright (c) 2025 R3BL LLC. Licensed under Apache License, Version 2.0.

//! Line insertion and deletion operations.
//!
//! This module acts as a thin shim layer that delegates to the actual implementation.
//! Refer to the module-level documentation in the operations module for details on the
//! "shim → impl → test" architecture and naming conventions.
//!
//! **Related Files:**
//! - **Implementation**: [`impl_line_ops`] - Business logic with unit tests
//! - **Integration Tests**: [`test_line_ops`] - Full pipeline testing via public API
//!
//! # Testing Strategy
//!
//! **This shim layer intentionally has no direct unit tests.**
//!
//! This is a deliberate architectural decision: these functions are pure delegation
//! layers with no business logic. Testing is comprehensively handled by:
//! - **Unit tests** in the implementation layer (with `#[test]` functions)
//! - **Integration tests** in the conformance tests validating the full pipeline
//!
//! For the complete testing philosophy,
//! and rationale behind this approach.
//!
//! # Architecture Overview
//!
//! ```text
//! ╭─────────────────╮    ╭────────────────╮    ╭─────────────────╮    ╭──────────────╮
//! │ Child Process   │────► PTY Controller │────► VTE Parser      │────► OffscreenBuf │
//! │ (vim, bash...)  │    │ (byte stream)  │    │ (state machine) │    │ (terminal    │
//! ╰──────┬──────────╯    ╰────────────────╯    ╰───────┬─────────╯    │  buffer)     │
//!        │                                             │              ╰───────┬──────╯
//!        │                                             │                      │
//!        │                                    ╔════════▼════════╗             │
//!        │                                    ║ Perform Trait   ║             │
//!        │                                    ║ Implementation  ║             │
//!        │                                    ╚═════════════════╝             │
//!        │                                                                    │
//!        │                                    ╭─────────────────╮             │
//!        │                                    │ RenderPipeline  ◄─────────────╯
//!        │                                    │ paint()         │
//!        ╰────────────────────────────────────► Terminal Output │
//!                                             ╰─────────────────╯
//! ```
//!
//! # [`CSI`] Sequence Processing Flow
//!
//! ```text
//! Application sends "ESC [2L" (insert 2 lines)
//!         ↓
//!     PTY Controlled (escape sequence)
//!         ↓
//!     PTY Controller (byte stream) <- in process_manager.rs
//!         ↓
//!     VTE Parser (parses `ESC [`...char pattern)
//!         ↓
//!     csi_dispatch() [routes to modules below]
//!         ↓
//!     Route to operations module:
//!       - cursor_ops:: for movement (A,B,C,D,H)
//!       - scroll_ops:: for scrolling (S,T)
//!       - sgr_ops:: for styling (m)     ╭───────────╮
//!       - line_ops:: for lines (L,M) <- │THIS MODULE│
//!       - char_ops:: for chars (@,P,X)  ╰───────────╯
//!         ↓
//!     Update OffscreenBuffer state
//! ```
//!
//! # [`VT-100`] Protocol Conventions
//!
//! This shim layer sits at the boundary between [`VT-100`] wire format and internal
//! types.
//!
//! ## Parameter Handling
//!
//! **Missing or zero parameters default to 1:**
//! - `ESC [L` (missing param) → insert 1 line
//! - `ESC [0L` (explicit zero) → insert 1 line
//! - `ESC [3L` (explicit value) → insert 3 lines
//!
//! This is handled by [`extract_nth_single_non_zero()`] which returns [`NonZeroU16`].
//!
//! ## Scroll Region Interaction
//!
//! Line insertion and deletion operations interact with the scrolling region set by
//! [`DECSTBM`]. Lines are shifted within the region boundaries, with new/blank lines
//! appearing at the opposite end.
//!
//! [`CSI`]: crate::CsiSequence
//! [`DECSTBM`]: https://vt100.net/docs/vt510-rm/DECSTBM.html
//! [`extract_nth_single_non_zero()`]: crate::ParamsExt::extract_nth_single_non_zero
//! [`impl_line_ops`]: crate::vt_100_ansi_impl::vt_100_impl_line_ops
//! [`NonZeroU16`]: std::num::NonZeroU16
//! [`test_line_ops`]: crate::vt_100_pty_output_conformance_tests::tests::vt_100_test_line_ops
//! [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
//! [module-level documentation]: self

use super::super::ansi_parser_public_api::AnsiToOfsBufPerformer;
use crate::{EraseDisplayMode, EraseLineMode, ParamsExt};

/// Handle IL (Insert Line) - insert n blank lines at cursor position.
/// Lines below cursor and within scroll region shift down.
///
/// **[`VT-100`] Protocol**: See [module-level documentation] for parameter handling
/// (missing/zero parameters default to 1) and scroll region interaction.
///
/// This operation respects [`VT-100`] scroll region boundaries.
/// See [`OffscreenBuffer::insert_lines_at`] for detailed behavior and scroll region
/// handling.
///
/// [`OffscreenBuffer::insert_lines_at`]: crate::OffscreenBuffer::insert_lines_at
/// [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
/// [module-level documentation]: self
pub fn insert_lines(performer: &mut AnsiToOfsBufPerformer, params: &vte::Params) {
    let how_many = params.extract_nth_single_non_zero(0).get().into();
    let at = performer.ofs_buf.cursor_pos.row_index;
    let result = performer.ofs_buf.insert_lines_at(at, how_many);
    debug_assert!(
        result.is_ok(),
        "Failed to insert {how_many:?} lines at row {at:?}",
    );
}

/// Handle DL (Delete Line) - delete n lines starting at cursor position.
/// Lines below cursor and within scroll region shift up.
/// Blank lines are added at the bottom of the scroll region.
///
/// **[`VT-100`] Protocol**: See [module-level documentation] for parameter handling
/// (missing/zero parameters default to 1) and scroll region interaction.
///
/// This operation respects [`VT-100`] scroll region boundaries.
/// See [`OffscreenBuffer::delete_lines_at`] for detailed behavior and scroll region
/// handling.
///
/// [`OffscreenBuffer::delete_lines_at`]: crate::OffscreenBuffer::delete_lines_at
/// [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
/// [module-level documentation]: self
pub fn delete_lines(performer: &mut AnsiToOfsBufPerformer, params: &vte::Params) {
    let how_many = params.extract_nth_single_non_zero(0).get().into();
    let at = performer.ofs_buf.cursor_pos.row_index;
    let result = performer.ofs_buf.delete_lines_at(at, how_many);
    debug_assert!(
        result.is_ok(),
        "Failed to delete {how_many:?} lines at row {at:?}",
    );
}

/// Handle EL (Erase Line) - erase part or all of current line.
///
/// Fills affected cells with spaces using the current SGR style (preserving active
/// background color). This differs from [`clear_line`] which fills with unstyled
/// [`PixelChar::Spacer`].
///
/// **[`VT-100`] Protocol**: The mode parameter selects the erase region:
/// - `0` (default, missing) - From cursor to end of line
/// - `1` - From start of line to cursor
/// - `2` - Entire line
///
/// [`PixelChar::Spacer`]: crate::PixelChar::Spacer
/// [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
/// [`clear_line`]: crate::OffscreenBuffer::clear_line
pub fn erase_line(performer: &mut AnsiToOfsBufPerformer, params: &vte::Params) {
    let mode: EraseLineMode = params
        .extract_nth_single_opt_raw(0)
        .unwrap_or(0)
        .into();
    let result = performer.ofs_buf.erase_line(mode);
    debug_assert!(result.is_ok(), "Failed to erase line in mode {mode:?}");
}

/// Handle ED (Erase Display) - erase part or all of the display.
///
/// Fills affected cells with spaces using the current SGR style (preserving active
/// background color).
///
/// **[`VT-100`] Protocol**: The mode parameter selects the erase region:
/// - `0` (default, missing) - From cursor to end of display
/// - `1` - From start of display to cursor
/// - `2` - Entire display
/// - `3` - Entire display and scrollback buffer (treated same as `2`)
///
/// [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
pub fn erase_display(performer: &mut AnsiToOfsBufPerformer, params: &vte::Params) {
    let mode: EraseDisplayMode = params
        .extract_nth_single_opt_raw(0)
        .unwrap_or(0)
        .into();
    let result = performer.ofs_buf.erase_display(mode);
    debug_assert!(
        result.is_ok(),
        "Failed to erase display in mode {mode:?}"
    );
}
