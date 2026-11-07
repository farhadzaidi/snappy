//! One display's overlay: a borderless, non-activating, click-through panel whose
//! content is Core Animation layers, one shape and two text layers per zone. Only zones
//! whose content changed are redrawn.

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSFont, NSFontWeightMedium, NSFontWeightSemibold,
    NSPanel, NSView, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFType, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGPath};
use objc2_foundation::NSString;
use objc2_quartz_core::{CALayer, CAShapeLayer, CATextLayer, CATransaction, kCAAlignmentCenter};
use snappy_core::geometry::Rect;
use snappy_platform::{OverlayScene, OverlayZone};

use super::style::{
    CAPTION_FONT_SIZE, CORNER_RADIUS, NUMBER_SHADOW_BLUR, NUMBER_SHADOW_OPACITY, Paint, Tint, place_zone,
    zone_style,
};
use crate::coords::{flip_rect, layer_y};
use crate::displays::to_cg_rect;

pub struct Surface {
    panel: Retained<NSPanel>,
    root: Retained<CALayer>,
    work_area: Option<Rect>,
    zones: Vec<ZoneLayers>,
}

impl Surface {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let panel = make_panel(mtm);
        let root = CALayer::new();
        let view = NSView::new(mtm);
        view.setLayer(Some(&root));
        view.setWantsLayer(true);
        panel.setContentView(Some(&view));
        Self {
            panel,
            root,
            work_area: None,
            zones: Vec::new(),
        }
    }

    /// Brings the layers in line with `scene`, touching only what differs.
    pub fn update(&mut self, scene: &OverlayScene, primary_height: f64) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);

        let moved = self.work_area != Some(scene.work_area);
        if moved {
            let frame = flip_rect(scene.work_area, primary_height);
            self.panel.setFrame_display(to_cg_rect(frame), false);
            self.work_area = Some(scene.work_area);
        }
        self.match_zone_count(scene.zones.len());
        let drawn: Vec<Option<&OverlayZone>> =
            self.zones.iter().map(|layers| layers.drawn.as_ref()).collect();
        let stale = stale_zones(&drawn, &scene.zones, moved);
        let scale = self.panel.backingScaleFactor();
        for (index, (layers, zone)) in self.zones.iter_mut().zip(&scene.zones).enumerate() {
            if stale.contains(&index) {
                layers.draw(zone, scene.work_area, scale);
            }
        }

        CATransaction::commit();
    }

    fn match_zone_count(&mut self, count: usize) {
        while self.zones.len() > count {
            if let Some(layers) = self.zones.pop() {
                layers.remove();
            }
        }
        while self.zones.len() < count {
            self.zones.push(ZoneLayers::new(&self.root));
        }
    }

    pub fn is_visible(&self) -> bool {
        self.panel.isVisible()
    }

    pub fn show(&self) {
        if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
    }

    pub fn hide(&self) {
        self.panel.orderOut(None);
    }

    pub fn close(self) {
        self.panel.close();
    }
}

/// The zones to redraw for `zones`: all of them when the overlay moved, otherwise those
/// that differ from what was drawn.
fn stale_zones(drawn: &[Option<&OverlayZone>], zones: &[OverlayZone], moved: bool) -> Vec<usize> {
    zones
        .iter()
        .enumerate()
        .filter(|(index, zone)| moved || drawn.get(*index).copied().flatten() != Some(*zone))
        .map(|(index, _)| index)
        .collect()
}

fn make_panel(mtm: MainThreadMarker) -> Retained<NSPanel> {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the panel is owned by a `Surface` for its whole life, never released on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setIgnoresMouseEvents(true);
    panel.setHasShadow(false);
    panel.setLevel(NSFloatingWindowLevel);
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel
}

struct ZoneLayers {
    shape: Retained<CAShapeLayer>,
    number: Retained<CATextLayer>,
    caption: Retained<CATextLayer>,
    drawn: Option<OverlayZone>,
}

impl ZoneLayers {
    fn new(root: &CALayer) -> Self {
        let shape = CAShapeLayer::new();
        let number = text_layer();
        let caption = text_layer();
        root.addSublayer(&shape);
        root.addSublayer(&number);
        root.addSublayer(&caption);
        Self {
            shape,
            number,
            caption,
            drawn: None,
        }
    }

    fn remove(self) {
        self.shape.removeFromSuperlayer();
        self.number.removeFromSuperlayer();
        self.caption.removeFromSuperlayer();
    }

    fn draw(&mut self, zone: &OverlayZone, work_area: Rect, scale: CGFloat) {
        let placement = place_zone(zone, work_area);
        let container = work_area.height;
        let style = zone_style(zone);

        let shape = placement.shape;
        self.shape.setFrame(layer_rect(shape, container));
        let bounds = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(shape.width, shape.height));
        // SAFETY: a null transform means identity.
        let path =
            unsafe { CGPath::with_rounded_rect(bounds, CORNER_RADIUS, CORNER_RADIUS, std::ptr::null()) };
        self.shape.setPath(Some(&path));
        self.shape.setFillColor(Some(&color(style.fill)));
        self.shape.setStrokeColor(Some(&color(style.border)));
        self.shape.setLineWidth(style.border_width);

        // SAFETY: reading an immutable framework constant.
        let medium = unsafe { NSFontWeightMedium };
        set_text(
            &self.number,
            &zone.number.to_string(),
            placement.number_size,
            medium,
        );
        self.number.setFrame(layer_rect(placement.number, container));
        self.number.setContentsScale(scale);

        match placement.caption.zip(zone.caption) {
            Some((frame, caption)) => {
                // SAFETY: as above.
                let semibold = unsafe { NSFontWeightSemibold };
                set_text(&self.caption, caption.text(), CAPTION_FONT_SIZE, semibold);
                self.caption.setFrame(layer_rect(frame, container));
                self.caption.setContentsScale(scale);
                self.caption.setHidden(false);
            }
            None => self.caption.setHidden(true),
        }

        self.drawn = Some(zone.clone());
    }
}

fn text_layer() -> Retained<CATextLayer> {
    let layer = CATextLayer::new();
    // SAFETY: reading an immutable framework constant.
    layer.setAlignmentMode(unsafe { kCAAlignmentCenter });
    layer.setForegroundColor(Some(&NSColor::whiteColor().CGColor()));
    layer.setShadowColor(Some(&NSColor::blackColor().CGColor()));
    layer.setShadowOpacity(NUMBER_SHADOW_OPACITY);
    layer.setShadowRadius(NUMBER_SHADOW_BLUR);
    layer.setShadowOffset(CGSize::new(0.0, 0.0));
    layer
}

fn set_text(layer: &CATextLayer, text: &str, size: f64, weight: f64) {
    let string = NSString::from_str(text);
    let string: &AnyObject = &string;
    let font = NSFont::systemFontOfSize_weight(size, weight);
    // SAFETY: the string is an NSString and the font an NSFont, which CATextLayer accepts.
    unsafe {
        layer.setString(Some(string));
        layer.setFont(Some(as_cf(&*font)));
    }
    layer.setFontSize(size);
}

fn as_cf<T: Message>(object: &T) -> &CFType {
    // SAFETY: `NSFont` is toll-free bridged with `CTFont`, a Core Foundation type.
    unsafe { &*(object as *const T).cast::<CFType>() }
}

/// A rectangle measured from the overlay's top-left, in the root layer's bottom-up space.
fn layer_rect(rect: Rect, container_height: f64) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.x, layer_y(rect.y, rect.height, container_height)),
        CGSize::new(rect.width, rect.height),
    )
}

fn color(paint: Paint) -> Retained<CGColor> {
    let base = match paint.tint {
        Tint::Accent => NSColor::controlAccentColor(),
        Tint::Mint => NSColor::systemMintColor(),
        Tint::White => NSColor::whiteColor(),
    };
    base.colorWithAlphaComponent(paint.alpha).CGColor()
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

    use super::*;

    fn zone(number: usize, highlighted: bool) -> OverlayZone {
        OverlayZone {
            frame: Rect::new(12.0 + 714.0 * (number as f64 - 1.0), 37.0, 702.0, 851.0),
            number,
            highlighted,
            caption: None,
            swap_source: false,
        }
    }

    #[test]
    fn only_zones_whose_content_changed_are_redrawn() {
        let shown = [zone(1, true), zone(2, false)];
        let drawn: Vec<Option<&OverlayZone>> = shown.iter().map(Some).collect();
        assert!(stale_zones(&drawn, &shown, false).is_empty(), "unchanged content");

        let moved_highlight = [zone(1, false), zone(2, true)];
        assert_eq!(stale_zones(&drawn, &moved_highlight, false), [0, 1]);
        let one_change = [zone(1, false), zone(2, false)];
        assert_eq!(stale_zones(&drawn, &one_change, false), [0]);
        let captioned = [
            OverlayZone {
                caption: Some(Caption::Stack),
                ..zone(1, true)
            },
            zone(2, false),
        ];
        assert_eq!(stale_zones(&drawn, &captioned, false), [0], "a new caption");

        let more = [zone(1, true), zone(2, false), zone(3, false)];
        let with_new_layers = [Some(&shown[0]), Some(&shown[1]), None];
        assert_eq!(stale_zones(&with_new_layers, &more, false), [2]);

        assert_eq!(
            stale_zones(&drawn, &shown, true),
            [0, 1],
            "a moved overlay redraws everything"
        );
    }
}
