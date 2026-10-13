//! How wide the client draws text in the font its markup labels use
//! (`InterfaceFontMetrics`), so a line can be wrapped to a label's width
//! before it is bound: a label draws one line and neither wraps nor clips.
//!
//! The advances are those of the client's default interface font, one per
//! printable ASCII character (the glyph's width plus its bearings, which is
//! what the client adds up when it measures a string). A character outside
//! that range adds nothing, as the client skips a glyph the font lacks.

/// The height of one line of the font, in pixels.
pub const LINE_HEIGHT: f64 = 16.0;

#[rustfmt::skip]
const ADVANCES: [u8; 95] = [
    // space ! " # $ % & ' ( ) * + , - . /
    4, 3, 5, 5, 6, 9, 9, 3, 4, 4, 4, 6, 3, 3, 3, 4,
    // 0 1 2 3 4 5 6 7 8 9 : ; < = > ?
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 3, 3, 6, 6, 6, 5,
    // @ A B C D E F G H I J K L M N O
    7, 9, 7, 8, 9, 7, 6, 9, 9, 3, 3, 8, 7, 11, 9, 9,
    // P Q R S T U V W X Y Z [ \ ] ^ _
    7, 9, 7, 7, 7, 8, 9, 11, 7, 7, 7, 4, 7, 4, 6, 6,
    // ` a b c d e f g h i j k l m n o
    4, 5, 6, 6, 6, 6, 3, 7, 6, 3, 3, 6, 3, 9, 6, 7,
    // p q r s t u v w x y z { | } ~
    6, 6, 4, 5, 3, 6, 7, 9, 5, 7, 6, 3, 5, 3, 6,
];

/// How far the pen moves after one character, in pixels.
#[must_use]
pub fn advance(c: char) -> f64 {
    if (' '..='~').contains(&c) {
        f64::from(ADVANCES[c as usize - ' ' as usize])
    } else {
        0.0
    }
}

/// How wide a line of text draws, in pixels.
#[must_use]
pub fn measure(text: &str) -> f64 {
    text.chars().map(advance).sum()
}

/// Breaks text into lines no wider than the width, at spaces where it can;
/// a word wider than the whole width is broken where the width runs out.
/// Blank text is no lines.
#[must_use]
pub fn wrap(text: &str, width: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut line_width = 0.0;
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let word_width = measure(word);
        let mut space = if line.is_empty() { 0.0 } else { advance(' ') };
        if !line.is_empty() && line_width + space + word_width > width {
            lines.push(std::mem::take(&mut line));
            line_width = 0.0;
            space = 0.0;
        }
        if word_width > width {
            for c in word.chars() {
                let step = advance(c);
                if !line.is_empty() && line_width + step > width {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0.0;
                }
                line.push(c);
                line_width += step;
            }
            continue;
        }
        if space > 0.0 {
            line.push(' ');
        }
        line.push_str(word);
        line_width += space + word_width;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
