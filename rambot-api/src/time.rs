use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::iter::Peekable;
use std::num::ParseIntError;
use std::ops::{
    Add,
    AddAssign,
    Div,
    DivAssign,
    Mul,
    MulAssign,
    Neg,
    Sub,
    SubAssign
};
use std::str::{FromStr, Chars};

const MINUTES_PER_HOUR: i64 = 60;
const SECONDS_PER_MINUTE: i64 = 60;
const MILLISECONDS_PER_SECOND: i64 = 1000;

/// The number of samples read each millisecond according to the target sample
/// rate imposed by Discord.
pub const SAMPLES_PER_MILLISECOND: i64 = 48;

/// The number of samples read each second according to the target sample rate
/// imposed by Discord.
pub const SAMPLES_PER_SECOND: i64 =
    SAMPLES_PER_MILLISECOND * MILLISECONDS_PER_SECOND;

/// The number of samples read each minute according to the target sample rate
/// imposed by Discord.
pub const SAMPLES_PER_MINUTE: i64 = SAMPLES_PER_SECOND * SECONDS_PER_MINUTE;

/// The number of samples read each hour according to the target sample rate
/// imposed by Discord.
pub const SAMPLES_PER_HOUR: i64 = SAMPLES_PER_MINUTE * MINUTES_PER_HOUR;

/// An enumeration of the different errors that can occur when parsing
/// [SampleDuration]s.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseSampleDurationError {

    /// An amount integer could not be parsed. The underlying [ParseIntError]
    /// is provided.
    ParseIntError(ParseIntError),

    /// A unit annotated to an amount was not recognized. The suffix
    /// representing the unit is provided. Note that a missing unit also raises
    /// this error, where the invalid suffix wrapped in this instance is the
    /// empty string.
    InvalidUnit(String),

    /// An overflow at creation time occurred. That is, the total amount of
    /// samples as specified by the descriptor overflows an [i64].
    Overflow
}

impl From<ParseIntError> for ParseSampleDurationError {
    fn from(e: ParseIntError) -> ParseSampleDurationError {
        ParseSampleDurationError::ParseIntError(e)
    }
}

impl Display for ParseSampleDurationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ParseSampleDurationError::ParseIntError(e) =>
                write!(f, "Error parsing amount: {}.", e),
            ParseSampleDurationError::InvalidUnit(u) =>
                write!(f, "Invalid unit: {}.", u),
            ParseSampleDurationError::Overflow =>
                write!(f, "Delay too large.")
        }
    }
}

impl Error for ParseSampleDurationError { }

/// An enumeration of the different errors that can occur when handling
/// [SampleDuration]s.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleDurationError {

    /// A checked divide operation ([SampleDuration::checked_div]) was called
    /// where the RHS operand was zero.
    DivideByZero,

    /// Any checked operation caused the amount of samples stored in the result
    /// to overflow an [i64].
    Overflow
}

impl Display for SampleDurationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SampleDurationError::DivideByZero => write!(f, "divide by zero"),
            SampleDurationError::Overflow => write!(f, "overflow")
        }
    }
}

impl Error for SampleDurationError { }

/// Syntactic sugar for `Result<T, SampleDurationError>`.
pub type SampleDurationResult<T = SampleDuration> =
    Result<T, SampleDurationError>;

/// An amount of time measured in samples as the fundamental unit. A second is
/// represented by [SAMPLES_PER_SECOND] samples. Negative durations are
/// permitted. Ordinary operations are implemented by means of [Add], [Sub],
/// [Mul], and [Div], allowing the use of operators.
///
/// The associated string format for this type, which is applied in its
/// [Display] and [FromStr] implementations, consists of an arbitrary amount of
/// value-unit-pairs. The value is an integer and the unit is one of `h`, `m`,
/// `s`, `ms`, and `sam`, representing hours, minutes, seconds, milliseconds,
/// and samples respectively. Multiple of these pairs are concatenated, without
/// spaces. An example would be `1s500ms`, which represents 1 second and 500
/// milliseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct SampleDuration(i64);

impl SampleDuration {

    /// Zero duration, i.e. passes in an instant.
    pub const ZERO: SampleDuration = SampleDuration(0);

    /// The maximum (positive) duration that can be represented by this struct.
    pub const MAX: SampleDuration = SampleDuration(i64::MAX);

    /// The minimum (negative with highest absolute value) duration that can be
    /// represented by this struct.
    pub const MIN: SampleDuration = SampleDuration(i64::MIN);

    /// Creates a new sample duration from the total number of samples.
    pub const fn from_samples(samples: i64) -> SampleDuration {
        SampleDuration(samples)
    }

    /// Creates a new sample duration from the total number of milliseconds.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the number of samples within the
    /// given amount of milliseconds does not fit inside an [i64].
    pub fn from_milliseconds(milliseconds: i64) -> SampleDurationResult {
        let samples = milliseconds.checked_mul(SAMPLES_PER_MILLISECOND)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Creates a new sample duration from the total number of seconds.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the number of samples within the
    /// given amount of seconds does not fit inside an [i64].
    pub fn from_seconds(seconds: i64) -> SampleDurationResult {
        let samples = seconds.checked_mul(SAMPLES_PER_SECOND)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Creates a new sample duration from the total number of minutes.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the number of samples within the
    /// given amount of minutes does not fit inside an [i64].
    pub fn from_minutes(minutes: i64) -> SampleDurationResult {
        let samples = minutes.checked_mul(SAMPLES_PER_MINUTE)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Creates a new sample duration from the total number of hours.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the number of samples within the
    /// given amount of hours does not fit inside an [i64].
    pub fn from_hours(hours: i64) -> SampleDurationResult {
        let samples = hours.checked_mul(SAMPLES_PER_HOUR)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Gets the total amount of samples represented by this duration. If this
    /// is negative, then this instance represents a negative duration.
    pub const fn samples(self) -> i64 {
        self.0
    }

    /// Gets the amount of whole milliseconds that fit inside this duration. A
    /// negative amount of milliseconds is considered to fit if the absolute
    /// amount of milliseconds would fit inside the absolute value of the
    /// duration.
    pub const fn milliseconds(self) -> i64 {
        self.0 / SAMPLES_PER_MILLISECOND
    }

    /// Gets the amount of whole seconds that fit inside this duration. A
    /// negative amount of seconds is considered to fit if the absolute amount
    /// of seconds would fit inside the absolute value of the duration.
    pub const fn seconds(self) -> i64 {
        self.0 / SAMPLES_PER_SECOND
    }

    /// Gets the amount of whole minutes that fit inside this duration. A
    /// negative amount of minutes is considered to fit if the absolute amount
    /// of minutes would fit inside the absolute value of the duration.
    pub const fn minutes(self) -> i64 {
        self.0 / SAMPLES_PER_MINUTE
    }

    /// Gets the amount of whole hours that fit inside this duration. A
    /// negative amount of hours is considered to fit if the absolute amount of
    /// hours would fit inside the absolute value of the duration.
    pub const fn hours(self) -> i64 {
        self.0 / SAMPLES_PER_HOUR
    }

    /// Gets the amount of samples represented by this duration after
    /// subtracting all whole milliseconds that fit. A negative amount of
    /// milliseconds is considered to fit if the absolute amount of
    /// milliseconds would fit inside the absolute value of the duration.
    pub const fn sub_millisecond_samples(self) -> i64 {
        self.0 % SAMPLES_PER_MILLISECOND
    }

    /// Gets the amount of whole milliseconds that would fit inside this
    /// duration after subtracting all whole seconds. A negative amount of
    /// milliseconds or seconds is considered to fit if the absolute amount of
    /// milliseconds or seconds would fit inside the absolute value of the
    /// duration.
    pub const fn sub_second_milliseconds(self) -> i64 {
        self.milliseconds() % MILLISECONDS_PER_SECOND
    }

    /// Gets the amount of whole seconds that would fit inside this duration
    /// after subtracting all whole minutes. A negative amount of seconds or
    /// minutes is considered to fit if the absolute amount of seconds or
    /// minutes would fit inside the absolute value of the duration.
    pub const fn sub_minute_seconds(self) -> i64 {
        self.seconds() % SECONDS_PER_MINUTE
    }

    /// Gets the amount of whole minutes that would fit inside this duration
    /// after subtracting all whole hours. A negative amount of minutes or
    /// hours is considered to fit if the absolute amount of minutes or hours
    /// would fit inside the absolute value of the duration.
    pub const fn sub_hour_minutes(self) -> i64 {
        self.minutes() % MINUTES_PER_HOUR
    }

    /// Adds this sample duration to another and returns the result. Checks for
    /// overflow.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the sum of the samples represented
    /// by this duration and the given `rhs` does not fit inside an [i64].
    pub fn checked_add(self, rhs: SampleDuration) -> SampleDurationResult {
        let samples = self.0.checked_add(rhs.0)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Subtracts this sample duration to another and returns the result.
    /// Checks for overflow.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the difference between the samples
    /// represented by this duration and the given `rhs` does not fit inside an
    /// [i64].
    pub fn checked_sub(self, rhs: SampleDuration) -> SampleDurationResult {
        let samples = self.0.checked_sub(rhs.0)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Multiplies this sample duration by a factor and returns the result.
    /// Checks for overflow.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if the amount of the samples
    /// represented by this duration multiplied by `rhs` does not fit inside an
    /// [i64].
    pub fn checked_mul(self, rhs: i64) -> SampleDurationResult {
        let samples = self.0.checked_mul(rhs)
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Divides this sample duration by a divisor and returns the result.
    /// Checks for zero divisor.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::DivideByZero] if `rhs` is zero.
    pub fn checked_div(self, rhs: i64) -> SampleDurationResult {
        let samples = self.0.checked_div(rhs)
            .ok_or(SampleDurationError::DivideByZero)?;

        Ok(SampleDuration(samples))
    }

    /// Negates this sample duration. That is, if this is negative, returns a
    /// positive duration with the same absolute value. If this is positive,
    /// returns a negative duration with the same absolute value. Checks for
    /// overflow.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if this duration is
    /// [SampleDuration::MIN].
    pub fn checked_neg(self) -> SampleDurationResult {
        let samples = self.0.checked_neg()
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Computes the absolute (positive) value of this sample duration. That
    /// is, if this is negative, returns a positive duration with the same
    /// absolute value. If this is positive, returns this duration unchanged.
    /// Checks for overflow.
    ///
    /// # Errors
    ///
    /// * [SampleDurationError::Overflow] if this duration is
    /// [SampleDuration::MIN].
    pub fn checked_abs(self) -> SampleDurationResult {
        let samples = self.0.checked_abs()
            .ok_or(SampleDurationError::Overflow)?;

        Ok(SampleDuration(samples))
    }

    /// Computes the absolute (positive) value of this sample duration. That
    /// is, if this is negative, returns a positive duration with the same
    /// absolute value. If this is positive, returns this duration unchanged.
    /// Does not check for overflow.
    pub const fn abs(self) -> SampleDuration {
        SampleDuration(self.0.abs())
    }
}

impl AddAssign for SampleDuration {
    fn add_assign(&mut self, rhs: SampleDuration) {
        self.0 += rhs.0;
    }
}

impl AddAssign<&SampleDuration> for SampleDuration {
    fn add_assign(&mut self, rhs: &SampleDuration) {
        self.0 += rhs.0;
    }
}

impl Add for SampleDuration {
    type Output = SampleDuration;

    fn add(mut self, rhs: SampleDuration) -> SampleDuration {
        self += rhs;
        self
    }
}

impl Add<&SampleDuration> for SampleDuration {
    type Output = SampleDuration;

    fn add(mut self, rhs: &SampleDuration) -> SampleDuration {
        self += rhs;
        self
    }
}

impl Add for &SampleDuration {
    type Output = SampleDuration;

    fn add(self, rhs: &SampleDuration) -> SampleDuration {
        *self + rhs
    }
}

impl SubAssign for SampleDuration {
    fn sub_assign(&mut self, rhs: SampleDuration) {
        self.0 -= rhs.0;
    }
}

impl SubAssign<&SampleDuration> for SampleDuration {
    fn sub_assign(&mut self, rhs: &SampleDuration) {
        self.0 -= rhs.0;
    }
}

impl Sub for SampleDuration {
    type Output = SampleDuration;

    fn sub(mut self, rhs: SampleDuration) -> SampleDuration {
        self -= rhs;
        self
    }
}

impl Sub<&SampleDuration> for SampleDuration {
    type Output = SampleDuration;

    fn sub(mut self, rhs: &SampleDuration) -> SampleDuration {
        self -= rhs;
        self
    }
}

impl Sub for &SampleDuration {
    type Output = SampleDuration;

    fn sub(self, rhs: &SampleDuration) -> SampleDuration {
        *self - rhs
    }
}

impl MulAssign<i64> for SampleDuration {
    fn mul_assign(&mut self, rhs: i64) {
        self.0 *= rhs;
    }
}

impl Mul<i64> for SampleDuration {
    type Output = SampleDuration;

    fn mul(mut self, rhs: i64) -> SampleDuration {
        self *= rhs;
        self
    }
}

impl Mul<i64> for &SampleDuration {
    type Output = SampleDuration;

    fn mul(self, rhs: i64) -> SampleDuration {
        *self * rhs
    }
}

impl DivAssign<i64> for SampleDuration {
    fn div_assign(&mut self, rhs: i64) {
        self.0 /= rhs;
    }
}

impl Div<i64> for SampleDuration {
    type Output = SampleDuration;

    fn div(mut self, rhs: i64) -> SampleDuration {
        self /= rhs;
        self
    }
}

impl Div<i64> for &SampleDuration {
    type Output = SampleDuration;

    fn div(self, rhs: i64) -> SampleDuration {
        *self / rhs
    }
}

impl Neg for SampleDuration {
    type Output = SampleDuration;

    fn neg(self) -> SampleDuration {
        SampleDuration(-self.0)
    }
}

impl Neg for &SampleDuration {
    type Output = SampleDuration;

    fn neg(self) -> SampleDuration {
        SampleDuration(-self.0)
    }
}

fn fmt_part(f: &mut Formatter<'_>, amount: i64, suffix: &str, empty: &mut bool)
        -> fmt::Result {
    if amount != 0 {
        write!(f, "{}{}", amount, suffix)?;
        *empty = false;
    }

    Ok(())
}

const HOUR_SUFFIX: &str = "h";
const MINUTE_SUFFIX: &str = "m";
const SECOND_SUFFIX: &str = "s";
const MILLISECOND_SUFFIX: &str = "ms";
const SAMPLE_SUFFIX: &str = "sam";

impl Display for SampleDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let hours = self.hours();
        let minutes = self.sub_hour_minutes();
        let seconds = self.sub_minute_seconds();
        let milliseconds = self.sub_second_milliseconds();
        let samples = self.sub_millisecond_samples();
        let mut empty = true;

        fmt_part(f, hours, HOUR_SUFFIX, &mut empty)?;
        fmt_part(f, minutes, MINUTE_SUFFIX, &mut empty)?;
        fmt_part(f, seconds, SECOND_SUFFIX, &mut empty)?;
        fmt_part(f, milliseconds, MILLISECOND_SUFFIX, &mut empty)?;
        fmt_part(f, samples, SAMPLE_SUFFIX, &mut empty)?;

        if empty {
            write!(f, "0{}", SAMPLE_SUFFIX)?;
        }

        Ok(())
    }
}

fn collect_while<F>(chars: &mut Peekable<Chars<'_>>, pred: F) -> String
where
    F: Fn(char) -> bool
{
    let mut s = String::new();

    while let Some(&c) = chars.peek() {
        if !pred(c) {
            break;
        }

        chars.next();
        s.push(c);
    }

    s
}

impl FromStr for SampleDuration {
    type Err = ParseSampleDurationError;

    fn from_str(s: &str) -> Result<SampleDuration, ParseSampleDurationError> {
        let mut chars = s.chars().peekable();
        let mut duration = SampleDuration::ZERO;

        while chars.peek().is_some() {
            let number =
                collect_while(&mut chars, |c| c == '-' || c.is_numeric());
            let unit = collect_while(&mut chars, char::is_alphabetic);
            let amount = number.parse::<i64>()?;
            let delta = if unit == HOUR_SUFFIX {
                SampleDuration::from_hours(amount)
                    .map_err(|_| ParseSampleDurationError::Overflow)?
            }
            else if unit == MINUTE_SUFFIX {
                SampleDuration::from_minutes(amount)
                    .map_err(|_| ParseSampleDurationError::Overflow)?
            }
            else if unit == SECOND_SUFFIX {
                SampleDuration::from_seconds(amount)
                    .map_err(|_| ParseSampleDurationError::Overflow)?
            }
            else if unit == MILLISECOND_SUFFIX {
                SampleDuration::from_milliseconds(amount)
                    .map_err(|_| ParseSampleDurationError::Overflow)?
            }
            else if unit == SAMPLE_SUFFIX {
                SampleDuration::from_samples(amount)
            }
            else {
                return Err(ParseSampleDurationError::InvalidUnit(unit))
            };

            duration = duration.checked_add(delta)
                .map_err(|_| ParseSampleDurationError::Overflow)?;
        }

        Ok(duration)
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn parse_positive() {
        let expected =
            SampleDuration::from_samples(SAMPLES_PER_MILLISECOND * 1500);
        let parsed = "1s500ms0h".parse::<SampleDuration>().unwrap();

        assert_eq!(expected, parsed);
    }

    #[test]
    fn parse_negative() {
        let expected =
            SampleDuration::from_samples(SAMPLES_PER_SECOND * -3700);
        let parsed = "-1h-1m-40s".parse::<SampleDuration>().unwrap();

        assert_eq!(expected, parsed);
    }

    #[test]
    fn parse_mixed() {
        let expected = SampleDuration::from_samples(10);
        let parsed = "-1ms58sam".parse::<SampleDuration>().unwrap();

        assert_eq!(expected, parsed);
    }

    #[test]
    fn parse_barely_no_overflow() {
        "53375995583h".parse::<SampleDuration>().unwrap();
    }

    #[test]
    fn parse_overflow_single_term() {
        let e = "53375995584h".parse::<SampleDuration>().unwrap_err();

        assert_eq!(ParseSampleDurationError::Overflow, e);
    }

    #[test]
    fn parse_overflow_multiple_terms() {
        let e = "53375995583h3600s".parse::<SampleDuration>().unwrap_err();

        assert_eq!(ParseSampleDurationError::Overflow, e);
    }

    #[test]
    fn parse_negative_overflow() {
        let e = "-53375995584h".parse::<SampleDuration>().unwrap_err();

        assert_eq!(ParseSampleDurationError::Overflow, e);
    }

    #[test]
    fn parse_invalid_int() {
        let e = "1-1s".parse::<SampleDuration>().unwrap_err();
        let expected = "1-1".parse::<i64>().unwrap_err();

        assert_eq!(ParseSampleDurationError::ParseIntError(expected), e);
    }

    #[test]
    fn parse_invalid_unit() {
        let e = "1m1q1s".parse::<SampleDuration>().unwrap_err();

        assert_eq!(ParseSampleDurationError::InvalidUnit("q".to_owned()), e);
    }

    #[test]
    fn parse_missing_unit() {
        let e = "1m1".parse::<SampleDuration>().unwrap_err();

        assert_eq!(ParseSampleDurationError::InvalidUnit("".to_owned()), e);
    }

    #[test]
    fn format_zero() {
        let s = format!("{}", SampleDuration::ZERO);

        assert_eq!("0sam", s);
    }

    #[test]
    fn format_positive() {
        // A second contains 48000 samples, a minute 2880000, and an hour
        // 172800000. A billion samples therefore is 5 hours, 47 minutes, 13
        // seconds, 333 milliseconds, and 16 samples.

        let s = format!("{}", SampleDuration::from_samples(1_000_000_000));

        assert_eq!("5h47m13s333ms16sam", s);
    }

    #[test]
    fn format_negative() {
        let s = format!("{}", SampleDuration::from_samples(-1_000_000_000));

        assert_eq!("-5h-47m-13s-333ms-16sam", s);
    }
}
