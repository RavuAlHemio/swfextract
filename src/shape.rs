use std::fmt::Write;

use swf::{Color, FillStyle, Gradient, LineJoinStyle, Shape, ShapeRecord, Twips};
use sxd_document::Package;
use sxd_document::dom::{Document, Element};


fn write_rgba_as_css<W: Write>(color: &Color, mut write: W) {
    write!(
        write,
        "rgba({},{},{},{})",
        color.r, color.g, color.b, (color.a as f64) / 255.0,
    ).unwrap();
}

fn populate_gradient<'d>(g: &Gradient, document: Document<'d>, gradient: Element<'d>) {
    gradient.set_attribute_value(
        "gradientTransform",
        &format!(
            "matrix({}, {}, {}, {}, {}, {})",
            g.matrix.a, g.matrix.b, g.matrix.c, g.matrix.d, g.matrix.tx, g.matrix.ty,
        ),
    );
    for stop in &g.records {
        let stop_elem = document.create_element("stop");
        gradient.append_child(stop_elem);

        stop_elem.set_attribute_value(
            "offset",
            &format!("{}", (stop.ratio as f64) / 255.0),
        );
        stop_elem.set_attribute_value(
            "style",
            &format!(
                "stop-color:#{:02X}{:02X}{:02X};stop-opacity:{}",
                stop.color.r, stop.color.g, stop.color.b, (stop.color.a as f64) / 255.0,
            ),
        );
    }
}

fn write_fill_as_color<'d, W: Write>(
    fill_style: &FillStyle,
    document: Document<'d>,
    defs: Element<'d>,
    gradient_id: &mut usize,
    mut write: W,
) {
    match fill_style {
        FillStyle::Color(c) => {
            write_rgba_as_css(c, write);
        },
        FillStyle::LinearGradient(lg) => {
            let gradient = document.create_element("linearGradient");
            gradient.set_attribute_value("id", &format!("grad{}", *gradient_id));
            defs.append_child(gradient);

            populate_gradient(lg, document, gradient);

            write!(write, "url(#grad{})", gradient_id).unwrap();
            *gradient_id += 1;
        },
        FillStyle::RadialGradient(rg) => {
            let gradient = document.create_element("radialGradient");
            gradient.set_attribute_value("id", &format!("grad{}", *gradient_id));
            defs.append_child(gradient);

            populate_gradient(rg, document, gradient);

            write!(write, "url(#grad{})", gradient_id).unwrap();
            *gradient_id += 1;
        },
        _ => {
            // TODO
            write!(write, "black").unwrap();
        },
    }
}

fn write_line_join_style_css_attributes<W: Write>(join_style: &LineJoinStyle, mut write: W) {
    match join_style {
        LineJoinStyle::Bevel => write!(write, "stroke-linejoin: bevel").unwrap(),
        LineJoinStyle::Round => write!(write, "stroke-linejoin: round").unwrap(),
        LineJoinStyle::Miter(m) => write!(write, "stroke-linejoin: miter; stroke-miterlimit: {}", m).unwrap(),
    }
}

/// Twips to pixels.
fn tw2px(twips: Twips) -> f64 {
    (twips.get() as f64) / 20.0
}


pub(crate) fn shape_to_svg(shape: &Shape) -> String {
    let svg_package = Package::new();
    let svg_document = svg_package.as_document();

    let svg = svg_document.create_element("svg");
    svg_document.root().append_child(svg);
    svg.set_default_namespace_uri(Some("http://www.w3.org/2000/svg"));
    svg.set_attribute_value("viewBox", &format!(
        "{} {} {} {}",
        shape.shape_bounds.x_min,
        shape.shape_bounds.y_min,
        shape.shape_bounds.x_max,
        shape.shape_bounds.y_max,
    ));
    let width = shape.shape_bounds.x_max - shape.shape_bounds.x_min;
    let height = shape.shape_bounds.y_max - shape.shape_bounds.y_min;
    svg.set_attribute_value("width", &format!("{}px", tw2px(width)));
    svg.set_attribute_value("height", &format!("{}px", tw2px(height)));

    let defs = svg_document.create_element("defs");
    svg.append_child(defs);
    let mut gradient_index = 0;

    // assemble styles
    let mut styles = String::new();
    for (i, fill_style) in shape.styles.fill_styles.iter().enumerate() {
        if styles.len() > 0 {
            styles.push_str("\n");
        }
        write!(styles, ".f{} {{ fill: ", i+1).unwrap();
        write_fill_as_color(
            fill_style,
            svg_document,
            defs,
            &mut gradient_index,
            &mut styles,
        );
        write!(styles, "; }}").unwrap();
    }
    for (i, line_style) in shape.styles.line_styles.iter().enumerate() {
        if styles.len() > 0 {
            styles.push_str("\n");
        }
        write!(styles, ".l{} {{ stroke: ", i+1).unwrap();
        write_fill_as_color(
            line_style.fill_style(),
            svg_document,
            defs,
            &mut gradient_index,
            &mut styles,
        );
        write!(styles, ";").unwrap();

        write!(styles, " ").unwrap();
        write_line_join_style_css_attributes(&line_style.join_style(), &mut styles);
        write!(styles, ";").unwrap();

        write!(styles, " stroke-width: {}px;", tw2px(line_style.width())).unwrap();

        write!(styles, " }}").unwrap();
    }

    let style = svg_document.create_element("style");
    defs.append_child(style);
    style.set_text(&styles);

    let mut path = svg_document.create_element("path");
    let mut classes = String::new();
    if shape.styles.fill_styles.len() > 0 {
        if classes.len() > 0 {
            classes.push(' ');
        }
        classes.push_str("f1");
    }
    if shape.styles.line_styles.len() > 0 {
        if classes.len() > 0 {
            classes.push(' ');
        }
        classes.push_str("l1");
    }
    path.set_attribute_value("class", &classes);

    let mut current_path_data = String::new();
    let mut current_coords = (Twips::ZERO, Twips::ZERO);
    for record in &shape.shape {
        if current_path_data.len() > 0 {
            current_path_data.push(' ');
        }

        match record {
            ShapeRecord::StyleChange(sc) => {
                // finish current path
                if current_path_data.len() > 0 {
                    svg.append_child(path);
                    path.set_attribute_value("d", &current_path_data);
                    current_path_data.clear();

                    path = svg_document.create_element("path");
                }
                // otherwise, reuse current path element

                current_coords = (Twips::ZERO, Twips::ZERO);
                if let Some((x, y)) = sc.move_to {
                    current_coords.0 += x;
                    current_coords.1 += y;
                }
                write!(current_path_data, "M {} {}", current_coords.0, current_coords.1).unwrap();

                let mut classes = String::new();
                if let Some(fs) = sc.fill_style_0 {
                    if classes.len() > 0 {
                        classes.push(' ');
                    }
                    write!(classes, "f{}", fs).unwrap();
                }
                if let Some(ls) = sc.line_style {
                    if classes.len() > 0 {
                        classes.push(' ');
                    }
                    write!(classes, "l{}", ls).unwrap();
                }
                if classes.len() > 0 {
                    path.set_attribute_value("class", &classes);
                }
            },
            ShapeRecord::CurvedEdge { control_delta_x, control_delta_y, anchor_delta_x, anchor_delta_y } => {
                let cx = *control_delta_x;
                let cy = *control_delta_y;
                let ax = *control_delta_x + *anchor_delta_x;
                let ay = *control_delta_y + *anchor_delta_y;
                write!(current_path_data, "q {} {} {} {}", cx, cy, ax, ay).unwrap();
                current_coords.0 += ax;
                current_coords.0 += ay;
            },
            ShapeRecord::StraightEdge { delta_x, delta_y } => {
                write!(current_path_data, "l {} {}", delta_x, delta_y).unwrap();
                current_coords.0 += *delta_x;
                current_coords.1 += *delta_y;
            },
        }
    }

    if current_path_data.len() > 0 {
        svg.append_child(path);
        path.set_attribute_value("d", &current_path_data);
    }

    let mut buf = Vec::new();
    sxd_document::writer::format_document(&svg_document, &mut buf)
        .expect("failed to write SVG");
    String::from_utf8(buf)
        .expect("written SVG is not UTF-8?!")
}
