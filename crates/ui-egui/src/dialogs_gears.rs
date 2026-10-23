//! Insert and edit spur gears through the same command flow as other standard parts.

use std::sync::Arc;

use serde_json::{Value, json};
use solidcraft_engine::doc::Document;

use crate::SolveApp;
use crate::dialogs::Kind;
use crate::dialogs_part_size;
use crate::selection::SelInput;
use crate::ui_kit::row;

#[derive(Clone, Debug)]
pub struct Form {
    pub component: Option<u64>,
    pub module: String,
    pub teeth: String,
    pub face_width: String,
    pub bore: String,
    pub pressure_angle: String,
    pub backlash: String,
    pub root_fillet_radius: String,
    source: Option<Arc<Document>>,
    original: Option<[String; 7]>,
}

impl Form {
    fn defaults() -> Self {
        Self {
            component: None,
            module: "2".into(),
            teeth: "24".into(),
            face_width: "10".into(),
            bore: "8".into(),
            pressure_angle: "20".into(),
            backlash: "0".into(),
            root_fillet_radius: "0".into(),
            source: None,
            original: None,
        }
    }

    fn existing(app: &SolveApp, component: u64) -> Option<Self> {
        let part = app.session.doc.parts.get(&component)?;
        if part.family != "spur_gear" {
            return None;
        }

        let mut values: [Option<String>; 6] = [None, None, None, None, None, None];
        for field in part.size.split(';') {
            let (key, value) = field.split_once('=')?;
            let index = match key {
                "m" => 0,
                "z" => 1,
                "b" => 2,
                "pa" => 3,
                "bl" => 4,
                "fr" => 5,
                _ => return None,
            };
            if values[index].replace(value.to_owned()).is_some() {
                return None;
            }
        }
        let [Some(module), Some(teeth), Some(bore), Some(pressure_angle), backlash, root_fillet_radius] = values else {
            return None;
        };
        let backlash = backlash.unwrap_or_else(|| "0".into());
        let root_fillet_radius = root_fillet_radius.unwrap_or_else(|| "0".into());
        let face_width = part.length?.to_string();
        let original =
            [module.clone(), teeth.clone(), face_width.clone(), bore.clone(), pressure_angle.clone(), backlash.clone(), root_fillet_radius.clone()];
        Some(Self {
            component: Some(component),
            module,
            teeth,
            face_width,
            bore,
            pressure_angle,
            backlash,
            root_fillet_radius,
            source: Some(app.session.doc.clone()),
            original: Some(original),
        })
    }

    fn current(&self, app: &SolveApp) -> Result<(), String> {
        let Some(component) = self.component else {
            return Ok(());
        };
        if !self.source.as_ref().is_some_and(|source| Arc::ptr_eq(source, &app.session.doc))
            || app.session.doc.parts.get(&component).is_none_or(|part| part.family != "spur_gear")
        {
            return Err("The design changed. Reopen Edit Gear Parameters for the component.".into());
        }
        Ok(())
    }

    fn params(&self) -> Result<Value, String> {
        let parse = |label: &str, value: &str| {
            let parsed = value.trim().parse::<f64>().map_err(|_| format!("{label} must be a number"))?;
            if !parsed.is_finite() {
                return Err(format!("{label} must be finite"));
            }
            Ok(parsed)
        };
        let teeth = self.teeth.trim().parse::<u32>().map_err(|_| "Tooth count must be a positive integer")?;
        Ok(json!({
            "module": parse("Module", &self.module)?,
            "teeth": teeth,
            "height": parse("Gear height", &self.face_width)?,
            "bore": parse("Bore", &self.bore)?,
            "pressure_angle": parse("Pressure angle", &self.pressure_angle)?,
            "backlash": parse("Backlash", &self.backlash)?,
            "root_fillet_radius": parse("Root fillet radius", &self.root_fillet_radius)?
        }))
    }
}

pub fn selected_component(app: &SolveApp) -> Option<u64> {
    let component = dialogs_part_size::selected_component(app)?;
    app.session.doc.parts.get(&component).is_some_and(|part| part.family == "spur_gear").then_some(component)
}

pub fn edit_command(app: &SolveApp, component: u64) -> Option<&'static str> {
    let part = app.session.doc.parts.get(&component)?;
    Some(if part.family == "spur_gear" { "gears.resize" } else { "parts.resize" })
}

pub fn start(app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let form = match id {
        "gears.insert" => Form::defaults(),
        "gears.resize" => Form::existing(app, selected_component(app)?)?,
        _ => return None,
    };
    Some((Kind::Gear(form), vec![]))
}

pub fn open(app: &mut SolveApp, component: u64) {
    if Form::existing(app, component).is_none() {
        app.set_status("Select an inserted spur gear to edit its parameters", true);
        return;
    }
    app.session.selection.clear();
    app.tree.picked_components = vec![component];
    app.start("gears.resize");
}

pub fn rows(_app: &mut SolveApp, ui: &mut egui::Ui, form: &mut Form) -> bool {
    row(ui, "Module (mm)", |ui| crate::ui_kit::text_field(ui, &mut form.module, "e.g. 2").0);
    row(ui, "Tooth count", |ui| crate::ui_kit::text_field(ui, &mut form.teeth, "e.g. 24").0);
    row(ui, "Gear height (mm)", |ui| crate::ui_kit::text_field(ui, &mut form.face_width, "e.g. 10").0);
    row(ui, "Bore (mm)", |ui| crate::ui_kit::text_field(ui, &mut form.bore, "0 for none").0);
    row(ui, "Pressure angle (deg)", |ui| crate::ui_kit::text_field(ui, &mut form.pressure_angle, "e.g. 20").0);
    row(ui, "Backlash (mm)", |ui| crate::ui_kit::text_field(ui, &mut form.backlash, "0 for none").0);
    row(ui, "Root fillet radius (mm)", |ui| crate::ui_kit::text_field(ui, &mut form.root_fillet_radius, "0 for none").0);
    false
}

pub fn commands(app: &SolveApp, form: &Form) -> Result<Vec<(String, Value)>, String> {
    form.current(app)?;
    let mut params = form.params()?;
    let command = if let Some(component) = form.component {
        params["component"] = json!(component);
        "gears.resize"
    } else {
        "gears.insert"
    };
    if let Some(original) = &form.original {
        let current = [&form.module, &form.teeth, &form.face_width, &form.bore, &form.pressure_angle, &form.backlash, &form.root_fillet_radius];
        if current.iter().zip(original).all(|(now, before)| *now == before) {
            return Ok(Vec::new());
        }
    }
    Ok(vec![(command.into(), params)])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::Services;
    use crate::dialogs::apply_commands;
    use solidcraft_engine::Session;

    #[test]
    fn gear_insert_and_edit_dialog_keep_parameters_and_rebuild_the_component() {
        let mut app = SolveApp::new(Session::default(), Services::default());
        app.start("gears.insert");
        let Some(crate::dialogs::Kind::Gear(form)) = app.dialog.as_mut().map(|dialog| &mut dialog.kind) else {
            panic!("gear insert should open its parameter dialog");
        };
        form.module = "2".into();
        form.teeth = "12".into();
        form.face_width = "8".into();
        form.bore = "4".into();
        form.pressure_angle = "25".into();
        form.backlash = "-0.1".into();
        form.root_fillet_radius = "0.4".into();
        let commands = apply_commands(&app, app.dialog.as_ref().expect("gear dialog")).expect("valid gear form");
        for (id, params) in commands {
            app.run(&id, params).expect("insert gear");
        }
        crate::dialogs::cancel(&mut app);
        let component = app.session.doc.parts.iter().find(|(_, part)| part.family == "spur_gear").map(|(id, _)| *id).expect("inserted gear");

        let items = crate::context_menu::items(&app, &crate::context_menu::Target::Component { id: component });
        let edit = items.iter().find(|item| item.id == "gears.resize").expect("gear edit menu item");
        assert_eq!(edit.label, "Edit Gear Parameters");
        crate::context_menu::run_item(&mut app, edit, egui::Pos2::ZERO);
        assert!(apply_commands(&app, app.dialog.as_ref().expect("gear edit dialog")).expect("unchanged gear form").is_empty());
        let Some(crate::dialogs::Kind::Gear(form)) = app.dialog.as_mut().map(|dialog| &mut dialog.kind) else {
            panic!("gear resize should open its parameter dialog");
        };
        assert_eq!(form.teeth, "12");
        assert_eq!(form.pressure_angle, "25");
        assert_eq!(form.backlash, "-0.1");
        assert_eq!(form.root_fillet_radius, "0.4");
        form.teeth = "24".into();
        form.pressure_angle = "20".into();
        form.backlash = "-0.2".into();
        form.root_fillet_radius = "0.1".into();
        let commands = apply_commands(&app, app.dialog.as_ref().expect("gear resize dialog")).expect("valid changed gear form");
        for (id, params) in commands {
            app.run(&id, params).expect("resize gear");
        }
        assert!(app.session.doc.parts[&component].size.contains("z=24"));
        assert!(app.session.doc.parts[&component].size.contains("pa=20"));
        assert!(app.session.doc.parts[&component].size.contains("bl=-0.2"));
        assert!(app.session.doc.parts[&component].size.contains("fr=0.1"));

        let path = std::env::temp_dir().join(format!("solidcraft-gear-parameters-{}.solidcraft", std::process::id()));
        app.run("file.save", serde_json::json!({"path":path})).expect("save edited gear");
        let mut reopened = Session::default();
        reopened.execute("doc.open", &serde_json::json!({"path":path})).expect("reopen edited gear");
        let reopened = SolveApp::new(reopened, Services::default());
        let form = Form::existing(&reopened, component).expect("gear parameters persisted");
        assert_eq!(form.teeth, "24");
        assert_eq!(form.pressure_angle, "20");
        assert_eq!(form.backlash, "-0.2");
        assert_eq!(form.root_fillet_radius, "0.1");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("solidcraft.bak"));
    }
}
