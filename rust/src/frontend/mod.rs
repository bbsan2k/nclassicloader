//! Host-neutral frontend policies. Native resource ownership and event/clock
//! translation stay in the UI adapters; these modules also build without SDL.

pub mod catalog;
pub mod font;
pub mod frame_limit;
pub mod gamepad;
pub mod gameplay;
pub mod import_lifecycle;
pub mod language_navigation;
pub mod languages;
pub mod launcher_activity;
pub mod launcher_controller;
pub mod launcher_frame;
pub mod launcher_navigation;
pub mod library;
pub mod menu_input;
pub mod menu_navigation;
pub mod preferences;
pub mod requests;
pub mod skin_metadata;
pub mod suspend_navigation;
pub mod timing;
pub mod toolbar_navigation;
