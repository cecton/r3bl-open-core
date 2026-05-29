// Copyright (c) 2025 R3BL LLC. Licensed under Apache License, Version 2.0.

//! Mode setting operations (SM/RM).
//!
//! This module acts as a thin shim layer that delegates to the actual implementation.
//! Refer to the module-level documentation in the operations module for details on the
//! "shim → impl → test" architecture and naming conventions.
//!
//! **Related Files:**
//! - **Implementation**: [`impl_mode_ops`] - Business logic with unit tests
//! - **Integration Tests**: [`test_mode_ops`] - Full pipeline testing via public API
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
//! Application sends "ESC [?7h" (set autowrap mode)
//!         ↓
//!     PTY Controlled (escape sequence)
//!         ↓
//!     PTY Controller (byte stream) <- in process_manager.rs
//!         ↓
//!     VTE Parser (parses ESC [...char pattern)
//!         ↓
//!     csi_dispatch() [routes to modules below]
//!         ↓
//!     Route to operations module:
//!       - cursor_ops:: for movement (A,B,C,D,H)
//!       - scroll_ops:: for scrolling (S,T)
//!       - sgr_ops:: for styling (m)
//!       - line_ops:: for lines (L,M)
//!       - char_ops:: for chars (@,P,X)  ╭───────────╮
//!       - mode_ops:: for modes (h,l) <- │THIS MODULE│
//!         ↓                             ╰───────────╯
//!     Update OffscreenBuffer state
//! ```
//!
//! [`CSI`]: crate::CsiSequence
//! [`impl_mode_ops`]: crate::vt_100_ansi_impl::vt_100_impl_mode_ops
//! [`test_mode_ops`]: crate::vt_100_pty_output_conformance_tests::tests::vt_100_test_mode_ops
//! [module-level documentation]: self

use super::super::{PrivateModeType, ansi_parser_public_api::AnsiToOfsBufPerformer};
use vte::Params;

/// Handle Set Mode (`CSI h`) command.
/// Supports both standard modes and private modes (with ? prefix).
pub fn set_mode(
    performer: &mut AnsiToOfsBufPerformer,
    params: &Params,
    intermediates: &[u8],
) {
    let is_private_mode = intermediates.contains(&b'?');
    if is_private_mode {
        let mode = PrivateModeType::from(params);
        match mode {
            PrivateModeType::AutoWrap => {
                performer.ofs_buf.set_auto_wrap_mode(true);
            }
            PrivateModeType::AlternateScreenBuffer => {
                performer.ofs_buf.set_alternate_screen_mode(true);
            }
            _ => {
                tracing::warn!("CSI ?{}h: Unhandled private mode", mode.as_u16());
            }
        }
    } else {
        tracing::warn!("CSI h: Standard mode setting not implemented");
    }
}

/// Handle Reset Mode (`CSI l`) command.
/// Supports both standard modes and private modes (with ? prefix).
pub fn reset_mode(
    performer: &mut AnsiToOfsBufPerformer,
    params: &Params,
    intermediates: &[u8],
) {
    let is_private_mode = intermediates.contains(&b'?');
    if is_private_mode {
        let mode = PrivateModeType::from(params);
        match mode {
            PrivateModeType::AutoWrap => {
                performer.ofs_buf.set_auto_wrap_mode(false);
            }
            PrivateModeType::AlternateScreenBuffer => {
                performer.ofs_buf.set_alternate_screen_mode(false);
                performer.ofs_buf.clear();
            }
            _ => {
                tracing::warn!("CSI ?{}l: Unhandled private mode", mode.as_u16());
            }
        }
    } else {
        tracing::warn!("CSI l: Standard mode reset not implemented");
    }
}
