//! Diagrams and formulas to SVG. Blocking and GUI-free: callers choose the thread.
use latex_rust::{Dim, MathBox, MathStyle, SvgOptions};
use norito_model::structure::{Diagram, Extent};

/// Larger inputs are refused rather than rendered slowly.
pub const MAX_SOURCE: usize = 32_000;

/// An SVG document. Formulas also report their extent.
#[derive(Debug, Clone)]
pub struct Rendering {
    pub svg: String,
    pub extent: Option<Extent>,
}

pub fn svg(diagram: &Diagram) -> Result<Rendering, String> {
    let source = match diagram {
        Diagram::Mermaid(source) | Diagram::InlineMath(source) | Diagram::DisplayMath(source) => {
            source
        }
    };
    if source.len() > MAX_SOURCE {
        return Err("図が大きすぎます（32KBまで）。".into());
    }
    match diagram {
        Diagram::InlineMath(math) => formula(math, MathStyle::Text),
        Diagram::DisplayMath(math) => formula(math, MathStyle::Display),
        // The renderer is not hardened against every malformed input.
        Diagram::Mermaid(mermaid) => {
            let svg = std::panic::catch_unwind(|| mermaid_rs_renderer::render(mermaid))
                .map_err(|_| "図の描画に失敗しました。".to_owned())?
                .map_err(|e| e.to_string())?;
            Ok(Rendering { svg, extent: None })
        }
    }
}

/// Laid out at a 12pt em, drawn at 48pt so rasterizing at text size rounds little.
/// A margin keeps overhanging ink inside the canvas; the extent includes it.
fn formula(math: &str, style: MathStyle) -> Result<Rendering, String> {
    let font = latex_rust::MathFont::stix_two_math().map_err(|e| e.to_string())?;
    let ast = latex_rust::parse(math).map_err(|e| e.to_string())?;
    let tree = latex_rust::layout_with_em_size_pt(&ast, &font, style, &Dim::from_i64(12))
        .map_err(|e| e.to_string())?;
    let margin = Dim::ratio(1, 10);
    let mut padded = MathBox::hpack(vec![
        MathBox::kern(margin.clone()),
        tree,
        MathBox::kern(margin.clone()),
    ]);
    padded.height = &padded.height + &margin;
    padded.depth = &padded.depth + &margin;
    let options = SvgOptions {
        font_size_pt: Dim::from_i64(48),
        ..SvgOptions::new()
    };
    let svg = latex_rust::render_svg(&padded, &font, &options).map_err(|e| e.to_string())?;
    let em = |dim: &Dim| f32::from_bits(dim.to_ieee32_bits());
    Ok(Rendering {
        svg,
        extent: Some(Extent {
            width: em(&padded.width),
            height: em(&padded.height),
            depth: em(&padded.depth),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mermaid_produces_svg_and_reports_invalid_input() {
        let rendering = svg(&Diagram::Mermaid(
            "flowchart LR\n A[開始] --> B[終了]".into(),
        ))
        .unwrap();
        assert!(rendering.svg.contains("<svg"));
        assert!(rendering.svg.contains("開始"));
        assert!(rendering.extent.is_none());
        assert!(super::svg(&Diagram::Mermaid("this is not a diagram".into())).is_err());
        assert!(super::svg(&Diagram::Mermaid("a".repeat(MAX_SOURCE + 1))).is_err());
    }
    #[test]
    fn math_produces_paths_for_fractions() {
        assert!(
            svg(&Diagram::InlineMath(r"\frac{1}{2}".into()))
                .unwrap()
                .svg
                .contains("<path")
        );
    }
    #[test]
    fn display_math_is_set_larger_and_reports_its_baseline() {
        let fraction = r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}";
        let inline = svg(&Diagram::InlineMath(fraction.into()))
            .unwrap()
            .extent
            .unwrap();
        let display = svg(&Diagram::DisplayMath(fraction.into()))
            .unwrap()
            .extent
            .unwrap();
        assert!(display.width > inline.width);
        assert!(display.height + display.depth > inline.height + inline.depth);
        let squares = svg(&Diagram::InlineMath("x^2 + y^2".into()))
            .unwrap()
            .extent
            .unwrap();
        // Three ems of ink plus the margins; mostly above the baseline.
        assert!((3.0..3.5).contains(&squares.width));
        assert!(squares.height > squares.depth && squares.depth > 0.1);
    }
}
