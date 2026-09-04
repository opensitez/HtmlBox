//! The colour spaces CSS Color 4 §7-§10 and CSS Color 5 §3 define, and the
//! conversions from each into the sRGB bytes the painter needs.
//!
//! Every matrix here is the one printed in css-color-4 §17 ("Sample code for
//! colour conversions"). Numbers are carried in `f64`: the round trip through
//! Lab is a cube root followed by a 3x3 matrix and a gamma curve, and in `f32`
//! that drifts by more than a byte on saturated colours.

/// Linear-light sRGB to a gamma-encoded sRGB byte. css-color-4 §17.
fn srgb_encode(c: f64) -> u8 {
    let v = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A gamma-encoded sRGB channel (0-1) to linear light. The inverse of
/// `srgb_encode`, needed by any space that mixes in linear light.
pub fn srgb_decode(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn mat3(m: [f64; 9], v: [f64; 3]) -> [f64; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}

const XYZ_D65_TO_LINEAR_SRGB: [f64; 9] = [
    3.2409699419045226,
    -1.5373831775700939,
    -0.4986107602930034,
    -0.9692436362808796,
    1.8759675015077204,
    0.04155505740717559,
    0.05563007969699366,
    -0.20397695888897652,
    1.0569715142428786,
];

/// Bradford-adapted D50 to D65, css-color-4 §17.
const D50_TO_D65: [f64; 9] = [
    0.9554734527042182,
    -0.023098536874261423,
    0.0632593086610217,
    -0.028369706963208136,
    1.0099954580058226,
    0.021041398966943008,
    0.012314001688319899,
    -0.020507696433477912,
    1.3303659366080753,
];

const LINEAR_P3_TO_XYZ_D65: [f64; 9] = [
    0.4865709486482162,
    0.26566769316909306,
    0.1982172852343625,
    0.2289745640697488,
    0.6917385218365064,
    0.079286914093745,
    0.0000000000000000,
    0.04511338185890264,
    1.043944368900976,
];

/// Linear-light sRGB triple to bytes.
fn linear_srgb_to_bytes(rgb: [f64; 3]) -> (u8, u8, u8) {
    (
        srgb_encode(rgb[0]),
        srgb_encode(rgb[1]),
        srgb_encode(rgb[2]),
    )
}

/// Oklab to linear-light sRGB. css-color-4 §9.2.
pub fn oklab_to_linear_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3,
    ]
}

/// Linear-light sRGB to Oklab — the inverse used by `color-mix(in oklab, …)`.
pub fn linear_srgb_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let l = 0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2];
    let m = 0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2];
    let s = 0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2];
    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

pub fn oklab_to_bytes(l: f64, a: f64, b: f64) -> (u8, u8, u8) {
    linear_srgb_to_bytes(oklab_to_linear_srgb(l, a, b))
}

/// CIE Lab (D50 white, as CSS specifies) to linear-light sRGB.
/// css-color-4 §8.2 plus the D50 to D65 adaptation.
pub fn lab_to_bytes(l: f64, a: f64, b: f64) -> (u8, u8, u8) {
    const KAPPA: f64 = 24389.0 / 27.0;
    const EPSILON: f64 = 216.0 / 24389.0;
    // The D50 reference white css-color-4 §17 uses.
    const WHITE_D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let x = if fx.powi(3) > EPSILON {
        fx.powi(3)
    } else {
        (116.0 * fx - 16.0) / KAPPA
    };
    let y = if l > KAPPA * EPSILON {
        ((l + 16.0) / 116.0).powi(3)
    } else {
        l / KAPPA
    };
    let z = if fz.powi(3) > EPSILON {
        fz.powi(3)
    } else {
        (116.0 * fz - 16.0) / KAPPA
    };
    let xyz_d50 = [x * WHITE_D50[0], y * WHITE_D50[1], z * WHITE_D50[2]];
    let xyz_d65 = mat3(D50_TO_D65, xyz_d50);
    linear_srgb_to_bytes(mat3(XYZ_D65_TO_LINEAR_SRGB, xyz_d65))
}

/// A polar space (`lch`/`oklch`) to its rectangular pair. Hue is in degrees.
pub fn polar_to_rect(c: f64, h_deg: f64) -> (f64, f64) {
    let h = h_deg.to_radians();
    (c * h.cos(), c * h.sin())
}

/// One of the predefined spaces `color()` names, css-color-4 §10.
/// Returns the sRGB bytes for the three coordinates given.
pub fn predefined_space_to_bytes(space: &str, c: [f64; 3]) -> Option<(u8, u8, u8)> {
    let linear = match space {
        "srgb" => [srgb_decode(c[0]), srgb_decode(c[1]), srgb_decode(c[2])],
        "srgb-linear" => c,
        "display-p3" => {
            let lin = [srgb_decode(c[0]), srgb_decode(c[1]), srgb_decode(c[2])];
            mat3(XYZ_D65_TO_LINEAR_SRGB, mat3(LINEAR_P3_TO_XYZ_D65, lin))
        }
        "xyz" | "xyz-d65" => mat3(XYZ_D65_TO_LINEAR_SRGB, c),
        "xyz-d50" => mat3(XYZ_D65_TO_LINEAR_SRGB, mat3(D50_TO_D65, c)),
        _ => return None,
    };
    Some(linear_srgb_to_bytes(linear))
}

/// sRGB bytes to linear light — the entry point for mixing.
pub fn bytes_to_linear_srgb(r: u8, g: u8, b: u8) -> [f64; 3] {
    [
        srgb_decode(r as f64 / 255.0),
        srgb_decode(g as f64 / 255.0),
        srgb_decode(b as f64 / 255.0),
    ]
}
