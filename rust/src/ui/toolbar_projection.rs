use super::*;

impl App {
    pub(super) fn project_menu_toolbar(&self) -> crate::frontend::menu_toolbar::ToolbarScene {
        let icons = [
            "btn_display",
            "btn_option",
            "btn_language",
            "btn_copyright",
            "manual",
        ];
        let plans =
            crate::frontend::menu_toolbar::toolbar(&crate::frontend::menu_toolbar::ToolbarInput {
                hover: self.hover.clone(),
                panel: self.panel.clone(),
                keyboard_focus: self.menu_focus == MenuFocus::Toolbar,
                keyboard_index: self.toolbar_index,
            });
        let resources = std::array::from_fn(|index| {
            let mut resources = crate::frontend::menu_toolbar::ToolbarItemResources {
                active: self.art.sprite_source("btn_menubar_active"),
                icon: self.art.sprite_source(icons[index]),
                tooltip: Vec::new(),
            };
            let plan = plans[index];
            if plan.tooltip {
                let key = [
                    "sys_menubarU_cm_Display",
                    "sys_menubarU_cm_Option",
                    "sys_menubarU_cm_Language",
                    "sys_menubarU_cm_Copyright",
                    "sys_menubarU_cm_Manual",
                ][index];
                let label = self.art.translate(
                    key,
                    ["Display", "Settings", "Language", "Game info", "Help"][index],
                );
                let style = self
                    .art
                    .layout
                    .get("tooltip")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let body_y = style.get("bodyY").and_then(Value::as_f64).unwrap_or(122.0) as f32;
                let height = style.get("height").and_then(Value::as_f64).unwrap_or(66.0) as f32;
                let edge = style
                    .get("edgeWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(12.0) as f32;
                let minimum = style
                    .get("minimumWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(144.0) as f32;
                let text_y = self
                    .art
                    .layout
                    .pointer("/labels/tooltip/position/1")
                    .and_then(Value::as_f64)
                    .unwrap_or(128.0) as f32;
                let tint = self.art.palette_color(
                    "caption",
                    if self.art.nes {
                        Color::BLACK
                    } else {
                        Color::WHITE
                    },
                );
                let sources = crate::frontend::menu_toolbar::TooltipSources {
                    body: self.art.sprite_source("caption"),
                    left: self.art.sprite_source("caption_edge_left"),
                    right: self.art.sprite_source("caption_edge"),
                    arrow: self.art.sprite_source("caption_arrow"),
                };
                let layout = self.art.measure(&label, "s");
                let atlases = self.art.font_atlases();
                resources.tooltip = crate::frontend::menu_toolbar::tooltip(
                    plan.center_x,
                    crate::frontend::menu_toolbar::TooltipStyle {
                        body_y,
                        height,
                        edge_width: edge,
                        minimum_width: minimum,
                        text_y,
                    },
                    &layout,
                    &atlases,
                    &sources,
                    crate::frontend::menu_scene::Rgba {
                        r: tint.r,
                        g: tint.g,
                        b: tint.b,
                        a: tint.a,
                    },
                );
            }
            resources
        });
        crate::frontend::menu_toolbar::toolbar_scene(
            self.art.texture_bindings.generation(),
            &plans,
            &resources,
        )
    }
}
