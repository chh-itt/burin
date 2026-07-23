//! M3 Typescale — 5 levels × 3 sizes = 15 type tokens.

#[derive(Clone, Copy, Debug)]
pub struct M3TypeToken {
    pub size: f32,
    pub line_height: f32,
    pub letter_spacing: f32, // stored as em; multiply by font_size for pixel value
}

#[derive(Clone, Debug)]
pub struct Typescale {
    pub display: DisplayTypes,
    pub headline: HeadlineTypes,
    pub title: TitleTypes,
    pub label: LabelTypes,
    pub body: BodyTypes,
}

#[derive(Clone, Debug)]
pub struct DisplayTypes {
    pub large: M3TypeToken,
    pub medium: M3TypeToken,
    pub small: M3TypeToken,
}

#[derive(Clone, Debug)]
pub struct HeadlineTypes {
    pub large: M3TypeToken,
    pub medium: M3TypeToken,
    pub small: M3TypeToken,
}

#[derive(Clone, Debug)]
pub struct TitleTypes {
    pub large: M3TypeToken,
    pub medium: M3TypeToken,
    pub small: M3TypeToken,
}

#[derive(Clone, Debug)]
pub struct LabelTypes {
    pub large: M3TypeToken,
    pub medium: M3TypeToken,
    pub small: M3TypeToken,
}

#[derive(Clone, Debug)]
pub struct BodyTypes {
    pub large: M3TypeToken,
    pub medium: M3TypeToken,
    pub small: M3TypeToken,
}

impl Typescale {
    pub fn default() -> Self {
        Self {
            display: DisplayTypes {
                large: M3TypeToken {
                    size: 57.0,
                    line_height: 64.0,
                    letter_spacing: -0.25,
                },
                medium: M3TypeToken {
                    size: 45.0,
                    line_height: 52.0,
                    letter_spacing: 0.0,
                },
                small: M3TypeToken {
                    size: 36.0,
                    line_height: 44.0,
                    letter_spacing: 0.0,
                },
            },
            headline: HeadlineTypes {
                large: M3TypeToken {
                    size: 32.0,
                    line_height: 40.0,
                    letter_spacing: 0.0,
                },
                medium: M3TypeToken {
                    size: 28.0,
                    line_height: 36.0,
                    letter_spacing: 0.0,
                },
                small: M3TypeToken {
                    size: 24.0,
                    line_height: 32.0,
                    letter_spacing: 0.0,
                },
            },
            title: TitleTypes {
                large: M3TypeToken {
                    size: 22.0,
                    line_height: 28.0,
                    letter_spacing: 0.0,
                },
                medium: M3TypeToken {
                    size: 16.0,
                    line_height: 24.0,
                    letter_spacing: 0.15,
                },
                small: M3TypeToken {
                    size: 14.0,
                    line_height: 20.0,
                    letter_spacing: 0.1,
                },
            },
            label: LabelTypes {
                large: M3TypeToken {
                    size: 14.0,
                    line_height: 20.0,
                    letter_spacing: 0.1,
                },
                medium: M3TypeToken {
                    size: 12.0,
                    line_height: 16.0,
                    letter_spacing: 0.5,
                },
                small: M3TypeToken {
                    size: 11.0,
                    line_height: 16.0,
                    letter_spacing: 0.5,
                },
            },
            body: BodyTypes {
                large: M3TypeToken {
                    size: 16.0,
                    line_height: 24.0,
                    letter_spacing: 0.5,
                },
                medium: M3TypeToken {
                    size: 14.0,
                    line_height: 20.0,
                    letter_spacing: 0.25,
                },
                small: M3TypeToken {
                    size: 12.0,
                    line_height: 16.0,
                    letter_spacing: 0.4,
                },
            },
        }
    }
}
