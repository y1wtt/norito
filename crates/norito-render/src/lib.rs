//! Diagrams and formulas to SVG. Blocking and GUI-free: callers choose the thread.
use norito_model::structure::Diagram;

/// Larger inputs are refused rather than rendered slowly.
pub const MAX_SOURCE: usize = 32_000;

pub fn svg(diagram: &Diagram) -> Result<String, String> {
    let source = match diagram {
        Diagram::Mermaid(source) | Diagram::Math(source) => source,
    };
    if source.len() > MAX_SOURCE {
        return Err("図が大きすぎます（32KBまで）。".into());
    }
    match diagram {
        Diagram::Math(math) => {
            let font = latex_rust::MathFont::stix_two_math().map_err(|e| e.to_string())?;
            latex_rust::latex_to_svg(math, &font, &latex_rust::SvgOptions::new())
                .map_err(|e| e.to_string())
        }
        // The renderer is not hardened against every malformed input.
        Diagram::Mermaid(mermaid) => {
            std::panic::catch_unwind(|| mermaid_rs_renderer::render(mermaid))
                .map_err(|_| "図の描画に失敗しました。".to_owned())?
                .map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mermaid_produces_svg_and_reports_invalid_input() {
        let svg = svg(&Diagram::Mermaid(
            "flowchart LR\n A[開始] --> B[終了]".into(),
        ))
        .unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("開始"));
        assert!(super::svg(&Diagram::Mermaid("this is not a diagram".into())).is_err());
        assert!(super::svg(&Diagram::Mermaid("a".repeat(MAX_SOURCE + 1))).is_err());
    }
    #[test]
    fn math_produces_paths_for_fractions() {
        assert!(
            svg(&Diagram::Math(r"\frac{1}{2}".into()))
                .unwrap()
                .contains("<path")
        );
    }
}
