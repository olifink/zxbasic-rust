//! Spectrum-style number formatting for `PRINT` and `STR$`.

/// Formats `x` the way the ZX Spectrum prints numbers: integers in full,
/// other values rounded to 8 significant digits, no leading zero before the
/// decimal point (`.5`), and `E` notation for very large or small values.
pub fn format_number(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    if x.fract() == 0.0 && x.abs() < 1e13 {
        return format!("{}", x as i64);
    }

    // Round to 8 significant digits: "d.ddddddde<exp>".
    let sci = format!("{:.7e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let sign = if x < 0.0 { "-" } else { "" };

    if (-5..=12).contains(&exp) {
        let body = if exp >= 0 {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                format!("{digits:0<int_len$}")
            } else {
                format!("{}.{}", &digits[..int_len], &digits[int_len..])
            }
        } else {
            format!(".{}{}", "0".repeat((-exp - 1) as usize), digits)
        };
        format!("{sign}{body}")
    } else {
        let (first, rest) = digits.split_at(1);
        let frac = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let exp_sign = if exp < 0 { '-' } else { '+' };
        format!("{sign}{first}{frac}E{exp_sign}{}", exp.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::format_number as f;

    #[test]
    fn integers() {
        assert_eq!(f(0.0), "0");
        assert_eq!(f(42.0), "42");
        assert_eq!(f(-7.0), "-7");
        assert_eq!(f(123456789.0), "123456789");
    }

    #[test]
    fn fractions_drop_leading_zero() {
        assert_eq!(f(0.5), ".5");
        assert_eq!(f(-0.25), "-.25");
        assert_eq!(f(1.0 / 3.0), ".33333333");
        assert_eq!(f(2.0 / 3.0), ".66666667");
        assert_eq!(f(1.23456789123), "1.2345679");
        assert_eq!(f(0.00001), ".00001");
    }

    #[test]
    fn exponent_notation() {
        assert_eq!(f(1e20), "1E+20");
        assert_eq!(f(1.5e-7), "1.5E-7");
        assert_eq!(f(-2.5e15), "-2.5E+15");
    }

    #[test]
    fn rounding_carries() {
        assert_eq!(f(9.999999999), "10");
        assert_eq!(f(0.1 + 0.2), ".3");
    }
}
