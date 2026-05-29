use std::ops::Range;

pub struct StepRange {
    pub range: Range<usize>,
    pub step: usize,
}

pub struct Rect {
    pub x: StepRange,
    pub y: StepRange,
}

pub struct Circle {
    pub cx: usize,
    pub cy: usize,
    pub radius: usize,
}

pub enum Mask {
    Full,
    Rect(Rect),
    Circle(Circle),
}

impl StepRange {
    pub fn is_valid(&self) -> bool {
        self.range.start < self.range.end && self.step > 0
    }
    pub fn len(&self) -> usize {
        (self.range.end - self.range.start).div_ceil(self.step)
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.range.clone().step_by(self.step)
    }
}
impl StepRange {
    pub fn contains(&self, value: usize) -> bool {
        self.range.contains(&value) && (value - self.range.start) % self.step == 0
    }
}
impl Rect {
    pub fn width(&self) -> usize {
        self.x.len()
    }

    pub fn height(&self) -> usize {
        self.y.len()
    }
    pub fn contains(&self, x: usize, y: usize) -> bool {
        self.x.contains(x) && self.y.contains(y)
    }
}
impl Circle {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        let dx = x as isize - self.cx as isize;
        let dy = y as isize - self.cy as isize;

        dx * dx + dy * dy <= (self.radius * self.radius) as isize
    }
    pub fn bounds(&self, step: usize) -> Rect {
        Rect {
            x: StepRange {
                range: (self.cx.saturating_sub(self.radius))..(self.cx.saturating_add(self.radius)),
                step,
            },
            y: StepRange {
                range: (self.cy.saturating_sub(self.radius))..(self.cy.saturating_add(self.radius)),
                step,
            },
        }
    }
}

impl Mask {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        match self {
            Mask::Full => true,

            Mask::Rect(rect) => rect.contains(x, y),

            Mask::Circle(circle) => circle.contains(x, y),
        }
    }
}

// impl Mask {
//     pub fn pixels(
//         &self,
//         frame_width: usize,
//         frame_height: usize,
//     ) -> impl Iterator<Item = (usize, usize)> {
//     }
// }
