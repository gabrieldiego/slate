/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Contains files specific to the servoshell app for Desktop systems.

mod accelerated_gl_media;
pub(crate) mod app;
pub(crate) mod cli;
pub(crate) mod dialog;
pub(crate) mod event_loop;
#[cfg(feature = "gamepad")]
pub(crate) mod gamepad;
pub mod geometry;
pub(crate) mod gui;
pub(crate) mod headed_window;
mod headless_window;
mod key_bindings;
mod keyutils;
mod page_scripts;
mod protocols;
mod slate_theme;
mod tracing;
#[cfg(feature = "webxr")]
mod webxr;
