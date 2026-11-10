//! One display's marked windows: a panel of its own, apart from the zones' panel, so the
//! outlines stay while zones come and go. It is the same borderless, non-activating,
//! click-through panel, holding one shape layer per outline, and shows only while it has
//! some.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSPanel, NSView};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGPath;
use objc2_quartz_core::{CALayer, CAShapeLayer, CATransaction};
use snappy_core::geometry::Rect;
use snappy_platform::OverlayMarks;

use super::style::{MARK_BORDER, MARK_BORDER_WIDTH, MARK_CORNER_RADIUS, mark_path};
use super::surface::{color, layer_rect, make_panel};
use crate::coords::flip_rect;
use crate::displays::to_cg_rect;

pub struct MarkSurface {
    panel: Retained<NSPanel>,
    root: Retained<CALayer>,
    work_area: Option<Rect>,
    outlines: Vec<Retained<CAShapeLayer>>,
}

impl MarkSurface {
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
            outlines: Vec::new(),
        }
    }

    /// Outlines `marks`, showing the panel while there are any and hiding it otherwise.
    pub fn update(&mut self, marks: &OverlayMarks, primary_height: f64) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);

        if self.work_area != Some(marks.work_area) {
            let frame = flip_rect(marks.work_area, primary_height);
            self.panel.setFrame_display(to_cg_rect(frame), false);
            self.work_area = Some(marks.work_area);
        }
        while self.outlines.len() > marks.frames.len() {
            if let Some(layer) = self.outlines.pop() {
                layer.removeFromSuperlayer();
            }
        }
        while self.outlines.len() < marks.frames.len() {
            let layer = CAShapeLayer::new();
            layer.setFillColor(None);
            layer.setStrokeColor(Some(&color(MARK_BORDER)));
            layer.setLineWidth(MARK_BORDER_WIDTH);
            self.root.addSublayer(&layer);
            self.outlines.push(layer);
        }
        for (layer, frame) in self.outlines.iter().zip(&marks.frames) {
            let path = mark_path(*frame, marks.work_area);
            layer.setFrame(layer_rect(path, marks.work_area.height));
            let bounds = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(path.width, path.height));
            // SAFETY: a null transform means identity.
            let shape = unsafe {
                CGPath::with_rounded_rect(bounds, MARK_CORNER_RADIUS, MARK_CORNER_RADIUS, std::ptr::null())
            };
            layer.setPath(Some(&shape));
        }

        CATransaction::commit();
        if marks.frames.is_empty() {
            self.panel.orderOut(None);
        } else if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
    }

    pub fn close(self) {
        self.panel.close();
    }
}
