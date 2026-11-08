//! Numbers as JSON Schema reads them: an integer is any number with no fraction (`1.0` is one),
//! two numbers are equal by value whatever their spelling, and `multipleOf` divides exactly in
//! decimal (`0.0075` is a multiple of `0.0001`, which dividing the floats says it is not), as
//! jsonschema-rs and boon do.

use std::cmp::Ordering;
use std::fmt::Write as _;

use serde_json::{Number, Value};

/// Whether `value` is written as an integer, as draft 4 asks (`1.0` is not one).
pub(crate) fn is_written_integer(value: &Value) -> bool {
    matches!(value, Value::Number(n) if n.is_i64() || n.is_u64())
}

/// Whether `value` is a number with no fraction.
pub(crate) fn is_integer(value: &Value) -> bool {
    match value {
        Value::Number(n) => {
            n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
            false
        }
    }
}

/// JSON's equality: numbers by value, objects whatever their fields' order.
pub(crate) fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => numbers_eq(a, b),
        (Value::String(a), Value::String(b)) => same_text(a, b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| json_eq(a, b)))
        }
        (a, b) => a == b,
    }
}

/// Whether two strings are the same, eight bytes at a time and inline: an `enum`'s options are
/// short, and a call to the system's `memcmp` for each costs more than the comparison.
#[inline]
pub(crate) fn same_text(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    while let (Some((x, rest_a)), Some((y, rest_b))) =
        (a.split_first_chunk::<8>(), b.split_first_chunk::<8>())
    {
        if x != y {
            return false;
        }
        (a, b) = (rest_a, rest_b);
    }
    a.iter().zip(b).all(|(x, y)| x == y)
}

fn numbers_eq(a: &Number, b: &Number) -> bool {
    if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
        return a == b;
    }
    a.as_f64() == b.as_f64()
}

/// `a` beside `b`, exactly: integers as integers, an integer beside a float without rounding it
/// (`9007199254740993` is above `9007199254740992.0`); `None` beside NaN.
pub(crate) fn compare(a: &Number, b: &Number) -> Option<Ordering> {
    match (whole(a), whole(b)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        (Some(a), None) => against(a, b.as_f64()?),
        (None, Some(b)) => against(b, a.as_f64()?).map(Ordering::reverse),
        (None, None) => a.as_f64()?.partial_cmp(&b.as_f64()?),
    }
}

/// `n` when it is written as an integer.
pub(crate) fn whole(n: &Number) -> Option<i128> {
    n.as_i64()
        .map(i128::from)
        .or_else(|| n.as_u64().map(i128::from))
}

/// The integer `i` beside the float `f`.
fn against(i: i128, f: f64) -> Option<Ordering> {
    if f.is_nan() {
        return None;
    }
    let floor = f.floor();
    // Past an i128, or infinite: beyond every integer JSON writes, on its side of zero.
    let Some(low) = exact(floor) else {
        return Some(match f > 0.0 {
            true => Ordering::Less,
            false => Ordering::Greater,
        });
    };
    Some(match i.cmp(&low) {
        Ordering::Equal if f > floor => Ordering::Less,
        other @ (Ordering::Less | Ordering::Equal | Ordering::Greater) => other,
    })
}

/// A whole float as the integer it is, read from its bits; `None` for a fraction, NaN, an
/// infinity, or one past an i128.
pub(crate) fn exact(f: f64) -> Option<i128> {
    if !f.is_finite() || f.fract() != 0.0 {
        return None;
    }
    let bits = f.to_bits();
    let biased = i32::try_from((bits >> 52) & 0x7FF).ok()?;
    let fraction = bits & ((1 << 52) - 1);
    // A whole subnormal is zero; a normal number has its leading one.
    let mantissa = i128::from(match biased {
        0 => fraction,
        _ => fraction | (1 << 52),
    });
    let shift = biased - 1075;
    let magnitude = match u32::try_from(shift) {
        // 53 bits of mantissa moved left: past 127 they leave an i128.
        Ok(left) if left <= 74 => mantissa << left,
        Ok(_) => return None,
        // A whole number's low bits are zeros: shifting them out loses nothing.
        Err(_) => mantissa >> u32::try_from(-shift).ok()?.min(127),
    };
    Some(match bits >> 63 {
        1 => -magnitude,
        _ => magnitude,
    })
}

/// Whether `n` divides by `m` with no remainder: integers by their remainder, others exactly in
/// decimal where both fit, else by their quotient.
pub(crate) fn multiple(n: &Number, m: &Number) -> bool {
    if let (Some(n), Some(m)) = (n.as_i64(), m.as_i64()) {
        // A divisor of 0 (no schema's) divides nothing out; `checked_rem` spares `MIN % -1`.
        return n.checked_rem(m).is_none_or(|rest| rest == 0);
    }
    // A whole divisor, however written (`1.0`), of a whole number: the remainder, exactly.
    let whole_of = |x: &Number| whole(x).or_else(|| x.as_f64().and_then(exact));
    if let (Some(n), Some(m)) = (whole_of(n), whole_of(m)) {
        return n.checked_rem(m).is_none_or(|rest| rest == 0);
    }
    let (Some(nf), Some(mf)) = (n.as_f64(), m.as_f64()) else {
        return true;
    };
    if mf <= 0.0 {
        return true;
    }
    if let (Some((nm, nd)), Some((mm, md))) = (decimal(nf), decimal(mf))
        && mm != 0
    {
        // n / m = (nm * 10^md) / (mm * 10^nd), the shared powers of ten cancelled first.
        let shared = nd.min(md);
        let scale = |mantissa: i128, places: u32| {
            mantissa.checked_mul(10_i128.checked_pow(places - shared)?)
        };
        if let (Some(a), Some(b)) = (scale(nm, md), scale(mm, nd)) {
            return a % b == 0;
        }
    }
    (nf / mf).fract() == 0.0
}

/// `x` as its shortest decimal spelling, a mantissa and its places after the point (`0.0075` is
/// `(75, 4)`), written on the stack; `None` for one too long to fit.
fn decimal(x: f64) -> Option<(i128, u32)> {
    let mut buffer = Spelled::default();
    write!(buffer, "{x}").ok()?;
    let text = buffer.text()?;
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let places = u32::try_from(fraction.len()).ok()?;
    let mut mantissa: i128 = 0;
    for digit in whole
        .trim_start_matches('-')
        .bytes()
        .chain(fraction.bytes())
    {
        mantissa = mantissa
            .checked_mul(10)?
            .checked_add(i128::from(digit.checked_sub(b'0')?))?;
    }
    Some((
        if text.starts_with('-') {
            -mantissa
        } else {
            mantissa
        },
        places,
    ))
}

/// A number's spelling, on the stack: 40 bytes hold any a mantissa of `i128` can.
struct Spelled {
    bytes: [u8; 40],
    len: usize,
}

impl Default for Spelled {
    fn default() -> Self {
        Self {
            bytes: [0; 40],
            len: 0,
        }
    }
}

impl Spelled {
    fn text(&self) -> Option<&str> {
        std::str::from_utf8(self.bytes.get(..self.len)?).ok()
    }
}

impl std::fmt::Write for Spelled {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len + s.len();
        let room = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        room.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
#[path = "number_tests.rs"]
mod tests;
