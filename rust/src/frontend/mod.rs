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
pub mod launcher_presentation;
pub mod launcher_scene;
pub mod launcher_status;
pub mod library;
pub mod menu_carousel;
pub mod menu_chrome;
pub mod menu_frame;
pub mod menu_input;
pub mod menu_motion;
pub mod menu_navigation;
pub mod menu_overlay;
pub mod menu_panel;
pub mod menu_scene;
pub mod menu_toolbar;
pub mod preferences;
pub mod requests;
pub mod skin_metadata;
pub mod suspend_navigation;
pub mod timing;
pub mod toolbar_navigation;
