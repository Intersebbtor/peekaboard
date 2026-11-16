//! The macOS menu bar icon, drawn like the Swift app's SplitKeyboardIcon: a
//! split keyboard (two schematic Sweep halves) with the layer number between
//! the halves. Black on transparent, shown as a template image, so it follows
//! the menu bar's appearance. Drawn with AppKit for the same font as Swift.

use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

/// Size with a single digit, in points; every further digit widens the icon
const WIDTH: f64 = 35.0;
const HEIGHT: f64 = 16.0;
/// The tray scales every icon to 18 pt high: 1 pt margin above and below
/// keeps the drawing at the Swift icon's 16 pt
const CANVAS_HEIGHT: f64 = 18.0;
const SCALE: f64 = 2.0;
const FONT_SIZE: f64 = 10.5;
/// NSFontWeightBold
const FONT_WEIGHT_BOLD: f64 = 0.4;

/// RGBA pixels (top row first), width and height in pixels. Call on the main
/// thread.
pub fn split_keyboard_icon(layer: Option<usize>) -> (Vec<u8>, u32, u32) {
    autoreleasepool(|_| unsafe { draw_icon(layer) }).unwrap_or_else(|| (vec![0; 4], 1, 1))
}

unsafe fn draw_icon(layer: Option<usize>) -> Option<(Vec<u8>, u32, u32)> {
    let text = NSString::from_str(&layer.map(|n| n.to_string()).unwrap_or_default());
    let font: Retained<AnyObject> =
        msg_send![class!(NSFont), monospacedDigitSystemFontOfSize: FONT_SIZE, weight: FONT_WEIGHT_BOLD];
    let black: Retained<AnyObject> = msg_send![class!(NSColor), blackColor];
    let attributes = attributes(&font, &black);

    let zero = NSString::from_str("0");
    let digit_width: NSSize = msg_send![&*zero, sizeWithAttributes: &*attributes];
    let extra_digits = text.length().saturating_sub(1) as f64;
    let width = WIDTH + extra_digits * digit_width.width;

    let pixels_wide = (width * SCALE).ceil() as isize;
    let pixels_high = (CANVAS_HEIGHT * SCALE) as isize;
    let color_space = NSString::from_str("NSDeviceRGBColorSpace");
    let rep: Allocated<AnyObject> = msg_send![class!(NSBitmapImageRep), alloc];
    let rep: Option<Retained<AnyObject>> = msg_send![
        rep,
        initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>(),
        pixelsWide: pixels_wide,
        pixelsHigh: pixels_high,
        bitsPerSample: 8isize,
        samplesPerPixel: 4isize,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: &*color_space,
        bytesPerRow: pixels_wide * 4,
        bitsPerPixel: 32isize
    ];
    let rep = rep?;
    // Points, so drawing happens at 2x
    let _: () = msg_send![&*rep, setSize: NSSize::new(width, CANVAS_HEIGHT)];
    let context: Option<Retained<AnyObject>> = msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: &*rep];
    let context = context?;

    let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
    let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: &*context];

    let margin: Retained<AnyObject> = msg_send![class!(NSAffineTransform), transform];
    let _: () = msg_send![&*margin, translateXBy: 0.0f64, yBy: (CANVAS_HEIGHT - HEIGHT) / 2.0];
    let _: () = msg_send![&*margin, concat];

    let _: () = msg_send![&*black, set];
    draw_left_half();
    // The right half is the left one mirrored
    let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
    let mirror: Retained<AnyObject> = msg_send![class!(NSAffineTransform), transform];
    let _: () = msg_send![&*mirror, translateXBy: width, yBy: 0.0f64];
    let _: () = msg_send![&*mirror, scaleXBy: -1.0f64, yBy: 1.0f64];
    let _: () = msg_send![&*mirror, concat];
    draw_left_half();
    let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];

    // Centre the digit's cap height, not the line box
    let text_size: NSSize = msg_send![&*text, sizeWithAttributes: &*attributes];
    let cap_height: f64 = msg_send![&*font, capHeight];
    let descender: f64 = msg_send![&*font, descender];
    let baseline = (HEIGHT - cap_height) / 2.0;
    let at = NSPoint::new((width - text_size.width) / 2.0, baseline + descender);
    let _: () = msg_send![&*text, drawAtPoint: at, withAttributes: &*attributes];

    let _: () = msg_send![&*context, flushGraphics];
    let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];

    let data: *const u8 = msg_send![&*rep, bitmapData];
    let bytes_per_row: isize = msg_send![&*rep, bytesPerRow];
    if data.is_null() {
        return None;
    }
    let (w, h, row) = (pixels_wide as usize, pixels_high as usize, bytes_per_row as usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        rgba.extend_from_slice(std::slice::from_raw_parts(data.add(y * row), w * 4));
    }
    Some((rgba, w as u32, h as u32))
}

unsafe fn attributes(font: &AnyObject, color: &AnyObject) -> Retained<AnyObject> {
    // NSFontAttributeName and NSForegroundColorAttributeName
    let keys = [NSString::from_str("NSFont"), NSString::from_str("NSColor")];
    let key_ptrs: [*const AnyObject; 2] = [
        Retained::as_ptr(&keys[0]).cast(),
        Retained::as_ptr(&keys[1]).cast(),
    ];
    let object_ptrs: [*const AnyObject; 2] = [font, color];
    msg_send![
        class!(NSDictionary),
        dictionaryWithObjects: object_ptrs.as_ptr(),
        forKeys: key_ptrs.as_ptr(),
        count: 2usize
    ]
}

/// A 3×3 key grid (standing for the 3×5 matrix) and a wide thumb key under
/// the inner columns
unsafe fn draw_left_half() {
    let key = 2.5;
    let gap = 0.8;
    let left = 1.0;
    let thumb_y = 1.6;
    let grid_y = thumb_y + key + 1.0;

    for row in 0..3 {
        for column in 0..3 {
            fill_key(NSRect::new(
                NSPoint::new(left + column as f64 * (key + gap), grid_y + row as f64 * (key + gap)),
                NSSize::new(key, key),
            ));
        }
    }

    let inner = left + 3.0 * key + 2.0 * gap;
    let thumb_width = 5.2;
    fill_key(NSRect::new(NSPoint::new(inner - thumb_width, thumb_y), NSSize::new(thumb_width, key)));
}

unsafe fn fill_key(rect: NSRect) {
    let path: Retained<AnyObject> =
        msg_send![class!(NSBezierPath), bezierPathWithRoundedRect: rect, xRadius: 0.5f64, yRadius: 0.5f64];
    let _: () = msg_send![&*path, fill];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns that hold any ink, as a string of # and .
    fn ink_columns(rgba: &[u8], w: usize, h: usize) -> String {
        (0..w)
            .map(|x| if (0..h).any(|y| rgba[(y * w + x) * 4 + 3] > 40) { '#' } else { '.' })
            .collect()
    }

    #[test]
    fn draws_two_halves_and_the_digit_between() {
        let (rgba, w, h) = split_keyboard_icon(Some(3));
        assert_eq!((w, h), (70, 36));
        let columns = ink_columns(&rgba, w as usize, h as usize);
        // Halves at the edges, digit in the middle, gaps in between
        assert!(columns.starts_with(".."), "{columns}");
        assert_eq!(&columns[34..36], "##", "digit in the middle: {columns}");
        let empty = split_keyboard_icon(None).0;
        let empty_columns = ink_columns(&empty, 70, 36);
        assert_eq!(&empty_columns[30..40], "..........", "no digit: {empty_columns}");
        // Mirrored halves
        let first = columns.find('#').unwrap();
        let last = columns.rfind('#').unwrap();
        assert_eq!(first, w as usize - 1 - last);
    }

    #[test]
    fn more_digits_widen_the_icon() {
        let (_, w, _) = split_keyboard_icon(Some(12));
        assert!(w > 70);
    }
}
