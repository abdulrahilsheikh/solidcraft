use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use super::CommandSpec;
use crate::params::{bad, num, str_, vec3};
use crate::{Result, Session};

const MIN_TEETH: u32 = 8;
const MAX_TEETH: u32 = 300;
const MAX_MODULE: f64 = 100.0;

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("gears.library", "Gear Library", library).icon("library").noundo().params("type? → supported gear types and parameters"),
    CommandSpec::new("gears.insert", "Insert Spur Gear", insert)
        .at("INSERT", "LIBRARY")
        .icon("gear")
        .params("module, teeth, height? (or face_width), bore?, pressure_angle?, backlash?, root_fillet_radius?, point?, name?"),
    CommandSpec::new("gears.resize", "Edit Gear Parameters", resize_atomic)
        .icon("scale")
        .params("component: id|name, module?, teeth?, height? (or face_width), bore?, pressure_angle?, backlash?, root_fillet_radius?"),
];

#[derive(Clone, Debug)]
struct Gear {
    module: f64,
    teeth: u32,
    face_width: f64,
    bore: f64,
    pressure_angle: f64,
    backlash: f64,
    root_fillet_radius: f64,
}

impl Gear {
    fn new(module: f64, teeth: u32, face_width: f64, bore: f64, pressure_angle: f64, backlash: f64, root_fillet_radius: f64) -> Self {
        Self { module, teeth, face_width, bore, pressure_angle, backlash, root_fillet_radius }
    }

    fn pitch_radius(&self) -> f64 {
        self.module * self.teeth as f64 / 2.0
    }

    fn outside_radius(&self) -> f64 {
        self.module * (self.teeth as f64 + 2.0) / 2.0
    }

    fn root_radius(&self) -> f64 {
        self.module * (self.teeth as f64 - 2.5) / 2.0
    }

    fn base_radius(&self) -> f64 {
        self.pitch_radius() * self.pressure_angle.to_radians().cos()
    }

    fn max_root_fillet_radius(&self) -> f64 {
        let root = self.root_radius();
        let base = self.base_radius();
        if base <= root { 0.0 } else { ((base * base - root * root) / (2.0 * root)).min(self.module / 2.0) }
    }

    fn validate(&self) -> std::result::Result<(), String> {
        if !self.module.is_finite() || self.module <= 0.0 || self.module > MAX_MODULE {
            return Err("module must be > 0 and <= 100 mm".into());
        }

        if !(MIN_TEETH..=MAX_TEETH).contains(&self.teeth) {
            return Err(format!("teeth must be between {MIN_TEETH} and {MAX_TEETH}"));
        }

        if !self.face_width.is_finite() || self.face_width <= 0.0 || self.face_width > 1000.0 {
            return Err("face_width must be > 0 and <= 1000 mm".into());
        }

        if !self.bore.is_finite() || self.bore < 0.0 || self.bore >= 2.0 * self.root_radius() {
            return Err("bore must be >= 0 and smaller than the root diameter".into());
        }

        if !self.pressure_angle.is_finite() || !(14.0..=35.0).contains(&self.pressure_angle) {
            return Err("pressure_angle must be between 14 and 35 degrees".into());
        }

        if !self.backlash.is_finite() || self.backlash.abs() >= std::f64::consts::PI * self.module / 2.0 {
            return Err("absolute backlash must be less than half the circular pitch".into());
        }

        if !self.root_fillet_radius.is_finite() || self.root_fillet_radius < 0.0 || self.root_fillet_radius > self.max_root_fillet_radius() {
            return Err(format!("root_fillet_radius must be between 0 and {} mm for these gear parameters", self.max_root_fillet_radius()));
        }

        Ok(())
    }

    // Compact persistence format for this workbench:
    // m=2;z=24;b=8;pa=20;bl=0;fr=0
    fn encode(&self) -> String {
        format!("m={};z={};b={};pa={};bl={};fr={}", self.module, self.teeth, self.bore, self.pressure_angle, self.backlash, self.root_fillet_radius)
    }

    fn decode(s: &str, face_width: f64) -> Option<Self> {
        let mut module = None;
        let mut teeth = None;
        let mut bore = None;
        let mut pressure_angle = None;
        let mut backlash = None;
        let mut root_fillet_radius = None;

        for field in s.split(';') {
            let (key, value) = field.split_once('=')?;
            match key {
                "m" => module = value.parse::<f64>().ok(),
                "z" => teeth = value.parse::<u32>().ok(),
                "b" => bore = value.parse::<f64>().ok(),
                "pa" => pressure_angle = value.parse::<f64>().ok(),
                "bl" => backlash = value.parse::<f64>().ok(),
                "fr" => root_fillet_radius = value.parse::<f64>().ok(),
                _ => return None,
            }
        }

        Some(Self::new(module?, teeth?, face_width, bore?, pressure_angle?, backlash.unwrap_or(0.0), root_fillet_radius.unwrap_or(0.0)))
    }
}

fn library(_s: &mut Session, p: &Value) -> Result<Value> {
    let kind = str_(p, "type").unwrap_or("spur");

    if !kind.eq_ignore_ascii_case("spur") {
        return Err(bad("gears.library", "only external spur gears are implemented"));
    }

    Ok(json!({
        "types": ["spur"],
        "units": "mm",
        "defaults": {
            "module": 2.0,
            "teeth": 24,
            "face_width": 10.0,
            "height": 10.0,
            "bore": 8.0,
            "pressure_angle": 20.0,
            "backlash": 0.0,
            "root_fillet_radius": 0.0
        },
        "limits": {
            "module": [0.1, MAX_MODULE],
            "teeth": [MIN_TEETH, MAX_TEETH],
            "face_width": [0.1, 1000.0],
            "height": [0.1, 1000.0],
            "pressure_angle": [14.0, 35.0],
            "backlash": ["greater than negative half the circular pitch", "less than half the circular pitch"],
            "root_fillet_radius": [0.0, "gear-dependent maximum"]
        },
        "geometry": {
            "pitch_diameter": "module * teeth",
            "outside_diameter": "module * (teeth + 2)",
            "root_diameter": "module * (teeth - 2.5)",
            "base_diameter": "pitch_diameter * cos(pressure_angle)"
        }
    }))
}

fn read_gear(p: &Value, previous: Option<&Gear>) -> Result<Gear> {
    let get = |key: &str, default: f64| num(p, key).unwrap_or(default);

    let module = get("module", previous.map_or(2.0, |g| g.module));

    let teeth_value = num(p, "teeth").unwrap_or(previous.map_or(24.0, |g| g.teeth as f64));

    if !teeth_value.is_finite() || teeth_value.fract() != 0.0 || teeth_value < 0.0 || teeth_value > u32::MAX as f64 {
        return Err(bad("gears", "teeth must be a positive integer"));
    }

    let face_width = if p.get("height").is_some() {
        get("height", previous.map_or(10.0, |g| g.face_width))
    } else {
        get("face_width", previous.map_or(10.0, |g| g.face_width))
    };

    let bore = get("bore", previous.map_or(8.0, |g| g.bore));

    let pressure_angle = get("pressure_angle", previous.map_or(20.0, |g| g.pressure_angle));

    let backlash = get("backlash", previous.map_or(0.0, |g| g.backlash));

    let root_fillet_radius = get("root_fillet_radius", previous.map_or(0.0, |g| g.root_fillet_radius));

    let gear = Gear::new(module, teeth_value as u32, face_width, bore, pressure_angle, backlash, root_fillet_radius);

    gear.validate().map_err(|e| bad("gears", e))?;
    Ok(gear)
}

fn involute_point(base_r: f64, t: f64) -> [f64; 2] {
    [base_r * (t.cos() + t * t.sin()), base_r * (t.sin() - t * t.cos())]
}

fn rotate_point(p: [f64; 2], a: f64) -> [f64; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn involute_t(base_r: f64, r: f64) -> f64 {
    if r <= base_r { 0.0 } else { ((r / base_r).powi(2) - 1.0).sqrt() }
}
/// Return a positive angular difference in [0, 2*PI).
fn positive_angle_delta(from: f64, to: f64) -> f64 {
    use std::f64::consts::TAU;

    (to - from).rem_euclid(TAU)
}

fn root_fillet_points(root_radius: f64, angle: f64, side: f64, radius: f64) -> Vec<[f64; 2]> {
    const SEGMENTS: usize = 8;

    let (sin, cos) = angle.sin_cos();
    let center_distance = root_radius + radius;
    let tangent_distance = (center_distance * center_distance - radius * radius).sqrt();
    let center = [tangent_distance * cos - side * radius * sin, tangent_distance * sin + side * radius * cos];
    let root_point = [root_radius * center[0] / center_distance, root_radius * center[1] / center_distance];
    let flank_point = [tangent_distance * cos, tangent_distance * sin];
    let start = (root_point[1] - center[1]).atan2(root_point[0] - center[0]);
    let end = (flank_point[1] - center[1]).atan2(flank_point[0] - center[0]);
    let sweep = if side > 0.0 { (end - start).rem_euclid(std::f64::consts::TAU) } else { -(start - end).rem_euclid(std::f64::consts::TAU) };

    (0..=SEGMENTS)
        .map(|i| {
            let a = start + sweep * i as f64 / SEGMENTS as f64;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// One external spur-gear tooth centered on +X.
///
/// Returns a closed polygon without repeating the first point.
/// The root ends extend into the root cylinder so the extruded
/// tooth can be joined to the cylinder.
fn one_tooth(g: &Gear) -> Vec<[f64; 2]> {
    use std::f64::consts::PI;

    const INVOLUTE_SEGMENTS: usize = 24;
    const TIP_ARC_SEGMENTS: usize = 12;

    let rb = g.base_radius();
    let ra = g.outside_radius();
    let rf = g.root_radius();

    let alpha = g.pressure_angle.to_radians();
    let half_pitch = PI / (2.0 * g.teeth as f64);

    // Involute parameter at the pitch and outside circles.
    let ta = involute_t(rb, ra);

    // inv(alpha) = tan(alpha) - alpha.
    // This aligns the tooth thickness at the pitch circle.
    let inv_alpha = alpha.tan() - alpha;
    let rotation = half_pitch + inv_alpha + g.backlash / (2.0 * g.pitch_radius());

    // Generate both flanks from the base circle to the tip.
    let mut left = Vec::with_capacity(INVOLUTE_SEGMENTS + 1);
    let mut right = Vec::with_capacity(INVOLUTE_SEGMENTS + 1);

    for i in 0..=INVOLUTE_SEGMENTS {
        let t = ta * i as f64 / INVOLUTE_SEGMENTS as f64;
        let p = involute_point(rb, t);

        left.push(rotate_point(p, -rotation));
        right.push(rotate_point([p[0], -p[1]], rotation));
    }

    let polar_angle = |p: &[f64; 2]| p[1].atan2(p[0]);

    let left_base_angle = polar_angle(&left[0]);
    let right_base_angle = polar_angle(&right[0]);

    let left_tip = left[INVOLUTE_SEGMENTS];
    let right_tip = right[INVOLUTE_SEGMENTS];

    let left_tip_angle = polar_angle(&left_tip);
    let right_tip_angle = polar_angle(&right_tip);

    // Build the boundary in order:
    //
    // left root -> left flank -> tip arc ->
    // right flank -> right root -> closing segment.
    let mut pts = Vec::with_capacity(2 * (INVOLUTE_SEGMENTS + 1) + TIP_ARC_SEGMENTS + 2);

    // Extend the flanks down to the root circle when the root
    // lies below the base circle. The closing chord lies inside
    // the root cylinder, where the tooth overlaps the cylinder.
    if rf < rb {
        if g.root_fillet_radius > 0.0 {
            pts.extend(root_fillet_points(rf, left_base_angle, -1.0, g.root_fillet_radius));
            pts.extend(left.iter().copied());
        } else {
            pts.push([rf * left_base_angle.cos(), rf * left_base_angle.sin()]);
            pts.extend(left.iter().copied());
        }
    } else {
        pts.extend(left.iter().copied());
    }

    // Connect the tooth tips with an outside-circle arc.
    // Unwrap the angular interval explicitly to avoid crossing
    // the -PI / +PI discontinuity.
    let tip_delta = positive_angle_delta(left_tip_angle, right_tip_angle);

    for i in 1..=TIP_ARC_SEGMENTS {
        let t = i as f64 / TIP_ARC_SEGMENTS as f64;
        let angle = left_tip_angle + tip_delta * t;

        pts.push([ra * angle.cos(), ra * angle.sin()]);
    }

    // Return down the right flank, excluding the tip because
    // the tip arc already terminates at the right tip.
    let right_flank = &right[..INVOLUTE_SEGMENTS];
    for p in right_flank.iter().rev() {
        pts.push(*p);
    }

    if rf < rb {
        if g.root_fillet_radius > 0.0 {
            pts.extend(root_fillet_points(rf, right_base_angle, 1.0, g.root_fillet_radius).into_iter().rev().skip(1));
        } else {
            pts.push([rf * right_base_angle.cos(), rf * right_base_angle.sin()]);
        }
    }

    // Remove adjacent duplicate points.
    let mut clean: Vec<[f64; 2]> = Vec::with_capacity(pts.len());

    for p in pts {
        if let Some(q) = clean.last() {
            let dx = p[0] - q[0];
            let dy = p[1] - q[1];

            if dx * dx + dy * dy < 1e-16 {
                continue;
            }
        }

        clean.push(p);
    }

    // The sketch API closes the profile itself. Do not return
    // a duplicate copy of the first vertex.
    if clean.len() > 1
        && let Some(last) = clean.last().copied()
    {
        let first = clean[0];

        let dx = first[0] - last[0];
        let dy = first[1] - last[1];

        if dx * dx + dy * dy < 1e-16 {
            clean.pop();
        }
    }

    clean
}
fn run(s: &mut Session, id: &str, p: Value) -> Result<Value> {
    s.execute(id, &p)
}

/// Build a gear at its local origin, with the bottom face at z = 0.
fn build(s: &mut Session, g: &Gear) -> Result<()> {
    let _frame = crate::frames::local_geometry();

    let root_d = 2.0 * g.root_radius();

    // Create the root cylinder.
    run(
        s,
        "solid.cylinder",
        json!({
            "diameter": format!("{root_d} mm"),
            "height": format!("{} mm", g.face_width),
            "base": [0, 0, 0],
            "axis": [0, 0, 1],
            "operation": "new"
        }),
    )?;

    // Create a sketch for one tooth.
    let points = one_tooth(g);

    run(
        s,
        "sketch.create",
        json!({
            "plane": "XY",
            "offset": "0 mm"
        }),
    )?;

    run(
        s,
        "sketch.line",
        json!({
            "points": points,
            "closed": true
        }),
    )?;

    run(s, "sketch.finish", json!({}))?;

    // Extrude the tooth and join it to the root cylinder.
    let val = json!({
        "distance": format!("{} mm", g.face_width),
        "direction": "positive",
        "operation": "join",
        "name":"Tooth",
    });
    run(s, "solid.extrude", val)?;

    // Circular pattern expects a list of feature references.
    run(
        s,
        "solid.pattern.circular",
        json!({
            "features":["Tooth"],
            "axis": {
                "origin": [0, 0, 0],
                "dir": [0, 0, 1]
            },
            "count": g.teeth,
            "angle": 360,
            "name": "Gear tooth pattern"
        }),
    )?;

    // Cut the bore after the pattern.
    if g.bore > 0.0 {
        run(
            s,
            "solid.cylinder",
            json!({
                "diameter": format!("{} mm", g.bore),
                "height": format!("{} mm", g.face_width + 2.0),
                "base": [0, 0, -1],
                "axis": [0, 0, 1],
                "operation": "cut"
            }),
        )?;
    }

    Ok(())
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let g = read_gear(p, None)?;

    let name = str_(p, "name").map(str::to_string).unwrap_or_else(|| format!("Spur Gear M{} Z{}", g.module, g.teeth));

    let active = s.active_component;

    let created = run(s, "component.create", json!({"name": name}))?;

    let component = created["component"].as_u64().ok_or_else(|| bad("gears.insert", "component creation returned no component id"))?;
    let occurrence = created["occurrence"].as_u64().ok_or_else(|| bad("gears.insert", "component creation returned no occurrence id"))?;

    build(s, &g)?;

    // Store the parameters so gears.resize can rebuild this gear.
    s.doc_mut()
        .parts
        .insert(component, solidcraft_doc::StandardPart { family: "spur_gear".to_string(), size: g.encode(), length: Some(g.face_width) });

    // Restore the caller's active component.
    run(s, "component.activate", json!({"component": active}))?;

    // Optional point placement. Directional placement and hole seating
    // can be added using the same transform/joint approach as parts.rs.
    if let Some(pt) = p.get("point").and_then(vec3)
        && let Some(o) = s.doc_mut().occurrences.iter_mut().find(|o| o.id == occurrence)
    {
        let mut m = solidcraft_doc::rigid(Vec3::ZERO, Vec3::ZERO, Vec3::X, 0.0);
        m[3] = [pt.x, pt.y, pt.z, 1.0];
        o.transform = m;
    }

    Ok(json!({
        "component": component,
        "occurrence": occurrence,
        "name": name,
        "type": "spur",
        "module": g.module,
        "teeth": g.teeth,
        "face_width": g.face_width,
        "height": g.face_width,
        "bore": g.bore,
        "backlash": g.backlash,
        "root_fillet_radius": g.root_fillet_radius,
        "pitch_diameter": g.pitch_radius() * 2.0,
        "outside_diameter": g.outside_radius() * 2.0,
        "root_diameter": g.root_radius() * 2.0,
        "base_diameter": g.base_radius() * 2.0
    }))
}

/// Keep a failed multi-feature rebuild isolated from the real session.
fn resize_atomic(s: &mut Session, p: &Value) -> Result<Value> {
    let mut computed = s.scratch();
    let result = resize(&mut computed, p)?;

    computed.undo.clear();
    computed.active_component = s.active_component;
    computed.active_occurrence_path = s.active_occurrence_path.clone();
    computed.active_sketch = s.active_sketch.filter(|id| computed.doc.feature(*id).is_some());

    s.accept_computed(computed);
    Ok(result)
}

fn resize(s: &mut Session, p: &Value) -> Result<Value> {
    let key = match p.get("component") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(name)) => name.clone(),
        _ => {
            return Err(bad("gears.resize", "`component` must be an id or name"));
        }
    };

    let component = s.doc.find_component(&key).ok_or_else(|| bad("gears.resize", format!("no component `{key}`")))?;

    let part = s
        .doc
        .parts
        .get(&component)
        .cloned()
        .filter(|part| part.family == "spur_gear")
        .ok_or_else(|| bad("gears.resize", "component is not a spur gear"))?;

    let old_width = part.length.unwrap_or(10.0);
    let old = Gear::decode(&part.size, old_width).ok_or_else(|| bad("gears.resize", "stored gear parameters are invalid"))?;

    let g = read_gear(p, Some(&old))?;

    // Remove this component's feature history in reverse order.
    let ids: Vec<u64> = s.doc.features.iter().filter(|f| f.component == component).map(|f| f.id).collect();

    for id in ids.iter().rev() {
        s.doc_mut().delete_feature(*id).map_err(|e| bad("gears.resize", format!("{e}")))?;
    }

    let active = s.active_component;

    run(s, "component.activate", json!({"component": component}))?;

    build(s, &g)?;

    s.doc_mut()
        .parts
        .insert(component, solidcraft_doc::StandardPart { family: "spur_gear".to_string(), size: g.encode(), length: Some(g.face_width) });

    let name = format!("Spur Gear M{} Z{}", g.module, g.teeth);

    if let Some(c) = s.doc_mut().components.iter_mut().find(|c| c.id == component) {
        c.name = name.clone();
    }

    run(s, "component.activate", json!({"component": active}))?;

    s.refresh();

    if let Some(error) = s.model.results.iter().find_map(|r| r.error.clone()) {
        return Err(crate::EngineError::Other(error));
    }

    Ok(json!({
        "component": component,
        "name": name,
        "module": g.module,
        "teeth": g.teeth,
        "face_width": g.face_width,
        "height": g.face_width,
        "bore": g.bore,
        "backlash": g.backlash,
        "root_fillet_radius": g.root_fillet_radius,
        "pitch_diameter": g.pitch_radius() * 2.0,
        "outside_diameter": g.outside_radius() * 2.0
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn gear_height_alias_and_backlash_are_read_and_validated() {
        let gear = read_gear(&json!({"height": 8.0, "backlash": 0.2}), None).unwrap();
        assert_eq!(gear.face_width, 8.0);
        assert_eq!(gear.backlash, 0.2);

        let legacy = read_gear(&json!({"face_width": 12.0}), None).unwrap();
        assert_eq!(legacy.face_width, 12.0);
        let previously_stored = Gear::decode("m=2;z=24;b=8;pa=20;bl=0", 10.0).unwrap();
        assert_eq!(previously_stored.backlash, 0.0);
        assert_eq!(previously_stored.root_fillet_radius, 0.0);

        assert_eq!(read_gear(&json!({"backlash": -0.1}), None).unwrap().backlash, -0.1);
        assert!(read_gear(&json!({"module": 2.0, "backlash": -std::f64::consts::PI}), None).is_err());
        assert!(read_gear(&json!({"root_fillet_radius": -0.1}), None).is_err());
        assert!(read_gear(&json!({"root_fillet_radius": 2.0}), None).is_err());
        assert!(read_gear(&json!({"module": 2.0, "backlash": std::f64::consts::PI}), None).is_err());
    }

    #[test]
    fn backlash_narrows_both_flanks_of_the_tooth() {
        let original = read_gear(&json!({"teeth": 8}), None).unwrap();
        let adjusted = read_gear(&json!({"teeth": 8, "backlash": 0.2}), None).unwrap();
        let original_tooth = one_tooth(&original);
        let adjusted_tooth = one_tooth(&adjusted);
        let angle = |point: [f64; 2]| point[1].atan2(point[0]);
        let half_shift = adjusted.backlash / (2.0 * adjusted.pitch_radius());
        let left_shift = angle(adjusted_tooth[0]) - angle(original_tooth[0]);
        let original_right = original_tooth[original_tooth.len() - 1];
        let adjusted_right = adjusted_tooth[adjusted_tooth.len() - 1];
        let right_shift = angle(adjusted_right) - angle(original_right);
        assert!((left_shift + half_shift).abs() < 1e-12);
        assert!((right_shift - half_shift).abs() < 1e-12);
    }

    #[test]
    fn root_fillet_profile_is_tangent_to_root_and_flank() {
        let gear = read_gear(&json!({"teeth": 24, "root_fillet_radius": 0.4}), None).unwrap();
        let rotation = std::f64::consts::PI / (2.0 * gear.teeth as f64) + gear.pressure_angle.to_radians().tan() - gear.pressure_angle.to_radians();
        let angle = -rotation;
        let arc = root_fillet_points(gear.root_radius(), angle, -1.0, gear.root_fillet_radius);
        let first = arc.first().copied().unwrap();
        let last = arc.last().copied().unwrap();
        assert!((first[0].hypot(first[1]) - gear.root_radius()).abs() < 1e-12);
        assert!(first[1].atan2(first[0]) < angle);
        let tangent_radius = (gear.root_radius().powi(2) + 2.0 * gear.root_radius() * gear.root_fillet_radius).sqrt();
        assert!((last[0].hypot(last[1]) - tangent_radius).abs() < 1e-12);
        let side = -1.0;
        let center = [
            tangent_radius * angle.cos() - side * gear.root_fillet_radius * angle.sin(),
            tangent_radius * angle.sin() + side * gear.root_fillet_radius * angle.cos(),
        ];
        assert!(arc.iter().all(|p| ((p[0] - center[0]).hypot(p[1] - center[1]) - gear.root_fillet_radius).abs() < 1e-12));

        let tooth = one_tooth(&gear);
        assert!(
            tooth
                .iter()
                .take(arc.len())
                .zip(&arc)
                .all(|(actual, expected)| { (actual[0] - expected[0]).abs() < 1e-12 && (actual[1] - expected[1]).abs() < 1e-12 })
        );
        let base = rotate_point(involute_point(gear.base_radius(), 0.0), angle);
        assert!((tooth[arc.len()][0] - base[0]).hypot(tooth[arc.len()][1] - base[1]) < 1e-12);
        let transition = [tooth[arc.len()][0] - last[0], tooth[arc.len()][1] - last[1]];
        let tangent = [angle.cos(), angle.sin()];
        assert!((transition[0] * tangent[1] - transition[1] * tangent[0]).abs() < 1e-12);
        let right_arc = root_fillet_points(gear.root_radius(), -angle, 1.0, gear.root_fillet_radius);
        assert!(arc.iter().zip(right_arc.iter()).all(|(left, right)| (left[0] - right[0]).abs() < 1e-12 && (left[1] + right[1]).abs() < 1e-12));
        assert!(tooth.len() > one_tooth(&read_gear(&json!({"teeth":24}), None).unwrap()).len());
    }
}
