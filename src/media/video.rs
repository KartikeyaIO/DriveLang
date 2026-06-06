use crate::media::frame::Frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeStamp {
    pub value: i64,
    pub num: u32,
    pub den: u32,
}
impl TimeStamp {
    pub fn to_seconds(&self) -> f64 {
        self.value as f64 * self.num as f64 / self.den as f64
    }
}
impl TimeStamp {
    pub fn from_seconds(seconds: f64, num: u32, den: u32) -> Self {
        let value = (seconds * den as f64 / num as f64) as i64;

        TimeStamp { value, num, den }
    }
}
impl TimeStamp {
    pub fn rescale(&self, new_num: u32, new_den: u32) -> Self {
        let new_value = (self.value as i128 * new_num as i128 * self.den as i128)
            / (self.num as i128 * new_den as i128);

        TimeStamp {
            value: new_value as i64,
            num: new_num,
            den: new_den,
        }
    }
}
impl TimeStamp {
    pub fn add(&self, other: TimeStamp) -> Self {
        let other = other.rescale(self.num, self.den);

        TimeStamp {
            value: self.value + other.value,
            num: self.num,
            den: self.den,
        }
    }
    pub fn default() -> Self {
        Self {
            value: 0,
            num: 1,
            den: 1,
        }
    }
}
pub struct VideoFrame {
    pub frame: Frame,
    pub pts: TimeStamp,
}
impl TimeStamp {
    pub fn sub(&self, other: TimeStamp) -> Self {
        let other = other.rescale(self.num, self.den);

        TimeStamp {
            value: self.value - other.value,
            num: self.num,
            den: self.den,
        }
    }

    pub fn zero_like(other: TimeStamp) -> Self {
        Self {
            value: 0,
            num: other.num,
            den: other.den,
        }
    }
}
