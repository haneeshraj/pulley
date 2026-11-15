//! pulley-scene v1: the only data format exchanged between Pulley components.
//!
//! Must match `schema/pulley-scene-v1.schema.json` exactly. Every struct denies unknown
//! fields. See `schema/SCENE.md` for conventions (scene space is pixels, y-down; tangents
//! are relative to their vertex; colors are gamma-encoded 0..1).

use serde::{Deserialize, Serialize};

pub const SCHEMA_NAME: &str = "pulley-scene";
/// The scene version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;
/// Oldest scene version this build still reads.
pub const MIN_SCHEMA_VERSION: u32 = 1;

fn one() -> f64 {
    1.0
}
fn is_one(v: &f64) -> bool {
    *v == 1.0
}
fn yes() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}
fn half() -> f64 {
    0.5
}
fn is_half(v: &f64) -> bool {
    *v == 0.5
}
fn four() -> f64 {
    4.0
}
fn is_four(v: &f64) -> bool {
    *v == 4.0
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Scene {
    pub schema: String,
    pub version: u32,
    pub source: Source,
    pub units: Units,
    pub color_space: ColorSpace,
    pub bounds: Rect,
    pub items: Vec<Item>,
    pub warnings: Vec<Warning>,
}

impl Scene {
    /// An empty scene with the current schema name and version.
    pub fn new(source: Source, units: Units, color_space: ColorSpace, bounds: Rect) -> Scene {
        Scene {
            schema: SCHEMA_NAME.to_string(),
            version: SCHEMA_VERSION,
            source,
            units,
            color_space,
            bounds,
            items: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceApp {
    Illustrator,
    Figma,
}

impl SourceApp {
    /// Name of the Pulley component that produces scenes for this app.
    pub fn component(self) -> &'static str {
        match self {
            SourceApp::Illustrator => "Pulley Illustrator panel",
            SourceApp::Figma => "Pulley Figma plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    Rgb,
    Cmyk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Source {
    pub app: SourceApp,
    pub app_version: String,
    pub pulley_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_color_mode: Option<ColorMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceUnit {
    Pt,
    Px,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Units {
    pub source: SourceUnit,
    pub px_per_source_unit: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpace {
    #[serde(rename = "srgb")]
    Srgb,
    #[serde(rename = "display-p3")]
    DisplayP3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    PassThrough,
    #[default]
    Normal,
    Darken,
    Multiply,
    LinearBurn,
    ColorBurn,
    Lighten,
    Screen,
    LinearDodge,
    ColorDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Item {
    Group(Group),
    Shape(Shape),
    Text(Text),
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Item::Group(g) => &g.id,
            Item::Shape(s) => &s.id,
            Item::Text(t) => &t.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Item::Group(g) => &g.name,
            Item::Shape(s) => &s.name,
            Item::Text(t) => &t.name,
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            Item::Group(g) => g.visible,
            Item::Shape(s) => s.visible,
            Item::Text(t) => t.visible,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub blend_mode: BlendMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Clip>,
    pub children: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Clip {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub paths: Vec<Path>,
    pub fill_rule: FillRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    #[default]
    Nonzero,
    Evenodd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Shape {
    pub id: String,
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub blend_mode: BlendMode,
    pub paths: Vec<Path>,
    pub fill_rule: FillRule,
    /// Bottom-most first, matching AE's stacking inside a shape group (later = on top).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Paint>,
    /// Bottom-most first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<Stroke>,
}

/// AE's path model: absolute vertices, tangents relative to their vertex.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Path {
    pub closed: bool,
    pub vertices: Vec<[f64; 2]>,
    pub in_tangents: Vec<[f64; 2]>,
    pub out_tangents: Vec<[f64; 2]>,
}

impl Path {
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GradientStop {
    pub offset: f64,
    pub color: [f64; 3],
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    #[serde(default = "half", skip_serializing_if = "is_half")]
    pub midpoint: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Paint {
    #[serde(rename_all = "camelCase")]
    Solid {
        color: [f64; 3],
        #[serde(default = "one", skip_serializing_if = "is_one")]
        opacity: f64,
    },
    #[serde(rename_all = "camelCase")]
    LinearGradient {
        stops: Vec<GradientStop>,
        start: [f64; 2],
        end: [f64; 2],
        #[serde(default = "one", skip_serializing_if = "is_one")]
        opacity: f64,
    },
    #[serde(rename_all = "camelCase")]
    RadialGradient {
        stops: Vec<GradientStop>,
        center: [f64; 2],
        radius_point: [f64; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focal_point: Option<[f64; 2]>,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        opacity: f64,
    },
}

impl Paint {
    pub fn opacity(&self) -> f64 {
        match self {
            Paint::Solid { opacity, .. }
            | Paint::LinearGradient { opacity, .. }
            | Paint::RadialGradient { opacity, .. } => *opacity,
        }
    }

    /// A representative solid color (first stop for gradients).
    pub fn representative_color(&self) -> [f64; 3] {
        match self {
            Paint::Solid { color, .. } => *color,
            Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
                stops.first().map(|s| s.color).unwrap_or([0.0, 0.0, 0.0])
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StrokeAlign {
    #[default]
    Center,
    Inside,
    Outside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cap: LineCap,
    #[serde(default, skip_serializing_if = "is_default")]
    pub join: LineJoin,
    #[serde(default = "four", skip_serializing_if = "is_four")]
    pub miter_limit: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dashes: Vec<f64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dash_offset: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub alignment: StrokeAlign,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FontRef {
    /// Empty when the source cannot provide one (Figma).
    pub post_script_name: String,
    pub family: String,
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextRun {
    /// UTF-16 code-unit offset (inclusive).
    pub start: u32,
    /// UTF-16 code-unit offset (exclusive).
    pub end: u32,
    pub font: FontRef,
    pub size: f64,
    /// Thousandths of an em.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tracking: f64,
    /// Line height in px; `None` = auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leading: Option<f64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub baseline_shift: f64,
    pub fill: Paint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextKind {
    Point,
    Area,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Justification {
    #[default]
    Left,
    Center,
    Right,
    JustifyLeft,
    JustifyCenter,
    JustifyRight,
    JustifyAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoxSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Text {
    pub id: String,
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub blend_mode: BlendMode,
    pub kind: TextKind,
    /// Paragraphs separated by `\r`.
    pub content: String,
    /// Text-local to scene space. Local origin: first baseline start (point text) or box
    /// top-left (area text).
    pub matrix: [f64; 6],
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub box_size: Option<BoxSize>,
    pub justification: Justification,
    pub runs: Vec<TextRun>,
    /// Fallback geometry: only `Item::Shape` entries are allowed.
    pub outlines: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Warning {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_name: Option<String>,
    pub message: String,
}

impl Warning {
    pub fn new(code: &str, item_id: Option<&str>, item_name: Option<&str>, message: impl Into<String>) -> Warning {
        Warning {
            code: code.to_string(),
            item_id: item_id.map(str::to_string),
            item_name: item_name.map(str::to_string),
            message: message.into(),
        }
    }
}
