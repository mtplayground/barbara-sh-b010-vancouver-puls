use anyhow::{Context, Result};

use crate::{
    drafts::PostDraft,
    retry::{is_transient_external_error, retry_transient, STORAGE_RETRY},
    storage::ObjectStorage,
};

const SVG_CONTENT_TYPE: &str = "image/svg+xml; charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedDraftAssets {
    pub post_asset_ref: String,
    pub reel_asset_ref: String,
}

pub async fn render_and_store_draft_assets(
    storage: &ObjectStorage,
    draft: &PostDraft,
) -> Result<RenderedDraftAssets> {
    if draft.id < 1 {
        anyhow::bail!("draft id must be positive");
    }

    let version = draft.updated_at.timestamp_millis();
    let post_key = format!("rendered/drafts/{}/{version}/post.svg", draft.id);
    let reel_key = format!("rendered/drafts/{}/{version}/reel.svg", draft.id);
    let post_svg = render_post_svg(draft);
    let reel_svg = render_reel_svg(draft);
    let post_bytes = post_svg.into_bytes();
    let reel_bytes = reel_svg.into_bytes();
    let post_object = retry_transient(
        STORAGE_RETRY,
        "store rendered post asset",
        |_| {
            let post_bytes = post_bytes.clone();
            let post_key = post_key.clone();
            async move {
                storage
                    .put_bytes(&post_key, post_bytes, Some(SVG_CONTENT_TYPE))
                    .await
            }
        },
        is_transient_external_error,
    )
    .await
    .with_context(|| {
        format!(
            "failed to store rendered post asset for draft `{}`",
            draft.id
        )
    })?;
    let reel_object = retry_transient(
        STORAGE_RETRY,
        "store rendered reel asset",
        |_| {
            let reel_bytes = reel_bytes.clone();
            let reel_key = reel_key.clone();
            async move {
                storage
                    .put_bytes(&reel_key, reel_bytes, Some(SVG_CONTENT_TYPE))
                    .await
            }
        },
        is_transient_external_error,
    )
    .await
    .with_context(|| {
        format!(
            "failed to store rendered reel asset for draft `{}`",
            draft.id
        )
    })?;

    Ok(RenderedDraftAssets {
        post_asset_ref: post_object.key,
        reel_asset_ref: reel_object.key,
    })
}

fn render_post_svg(draft: &PostDraft) -> String {
    render_svg(
        1080,
        1350,
        VisualLayout {
            label: "Vancouver Pulse",
            title: headline_from(&draft.caption_en, 74),
            subtitle: headline_from(&draft.caption_zh, 42),
            caption_en: &draft.caption_en,
            caption_zh: &draft.caption_zh,
            title_font_size: 74,
            subtitle_font_size: 42,
            caption_font_size: 31,
            top_band: 86,
            skyline_y: 488,
        },
    )
}

fn render_reel_svg(draft: &PostDraft) -> String {
    render_svg(
        1080,
        1920,
        VisualLayout {
            label: "Vancouver Pulse Reel",
            title: headline_from(&draft.caption_en, 68),
            subtitle: headline_from(&draft.caption_zh, 38),
            caption_en: &draft.caption_en,
            caption_zh: &draft.caption_zh,
            title_font_size: 78,
            subtitle_font_size: 44,
            caption_font_size: 34,
            top_band: 118,
            skyline_y: 720,
        },
    )
}

#[derive(Debug, Clone)]
struct VisualLayout<'a> {
    label: &'a str,
    title: String,
    subtitle: String,
    caption_en: &'a str,
    caption_zh: &'a str,
    title_font_size: i32,
    subtitle_font_size: i32,
    caption_font_size: i32,
    top_band: i32,
    skyline_y: i32,
}

fn render_svg(width: i32, height: i32, layout: VisualLayout<'_>) -> String {
    let title_lines = wrap_text(&layout.title, 18, 3);
    let subtitle_lines = wrap_text(&layout.subtitle, 16, 2);
    let en_lines = wrap_text(layout.caption_en, 44, 4);
    let zh_lines = wrap_text(layout.caption_zh, 28, 3);
    let title_y = layout.top_band + 112;
    let subtitle_y = title_y + (title_lines.len() as i32 * (layout.title_font_size + 9)) + 22;
    let caption_y = height - 322;
    let escaped_label = escape_xml(layout.label);

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="{escaped_label}">
<defs>
  <linearGradient id="sky" x1="0" x2="1" y1="0" y2="1">
    <stop offset="0%" stop-color="#00d2ff"/>
    <stop offset="40%" stop-color="#39ff88"/>
    <stop offset="100%" stop-color="#ff3d7f"/>
  </linearGradient>
  <linearGradient id="panel" x1="0" x2="0" y1="0" y2="1">
    <stop offset="0%" stop-color="#fff7d6"/>
    <stop offset="100%" stop-color="#ffffff"/>
  </linearGradient>
  <filter id="shadow" x="-10%" y="-10%" width="120%" height="120%">
    <feDropShadow dx="0" dy="16" stdDeviation="14" flood-color="#111827" flood-opacity="0.28"/>
  </filter>
</defs>
<rect width="{width}" height="{height}" fill="url(#sky)"/>
<circle cx="{accent_x}" cy="{accent_y}" r="245" fill="#ffe600" opacity="0.86"/>
<path d="M0 {mountain_y} C170 {mountain_a}, 250 {mountain_b}, 372 {mountain_y} C540 {mountain_c}, 620 {mountain_a}, 782 {mountain_y} C895 {mountain_b}, 984 {mountain_c}, {width} {mountain_y} L{width} {skyline_y} L0 {skyline_y} Z" fill="#ffffff" opacity="0.76"/>
<g transform="translate(0 {skyline_y})">
  <rect x="0" y="200" width="{width}" height="118" fill="#101827"/>
  <rect x="52" y="96" width="98" height="222" fill="#16213a"/>
  <rect x="178" y="20" width="116" height="298" fill="#243b72"/>
  <rect x="332" y="138" width="126" height="180" fill="#172554"/>
  <rect x="486" y="58" width="92" height="260" fill="#111827"/>
  <rect x="626" y="0" width="140" height="318" fill="#2563eb"/>
  <rect x="802" y="112" width="102" height="206" fill="#0f172a"/>
  <rect x="936" y="44" width="92" height="274" fill="#1f2937"/>
  <path d="M0 274 C230 230 398 338 606 282 C785 235 895 220 {width} 246 L{width} 318 L0 318 Z" fill="#00f5d4" opacity="0.86"/>
  <path d="M90 258 C270 232 430 282 612 250 C795 218 944 212 1030 230" fill="none" stroke="#fffb00" stroke-width="20" stroke-linecap="round"/>
  <circle cx="182" cy="230" r="17" fill="#ff3d7f"/>
  <circle cx="638" cy="244" r="17" fill="#ff3d7f"/>
  <circle cx="914" cy="218" r="17" fill="#ff3d7f"/>
</g>
<g filter="url(#shadow)">
  <rect x="54" y="58" width="{panel_width}" height="{panel_height}" rx="38" fill="url(#panel)" opacity="0.97"/>
  <rect x="54" y="58" width="{panel_width}" height="18" rx="9" fill="#ff3d7f"/>
  <rect x="86" y="90" width="172" height="44" rx="22" fill="#111827"/>
  <text x="112" y="120" font-family="Inter, Arial, sans-serif" font-size="22" font-weight="800" fill="#ffffff" letter-spacing="2">VANCOUVER</text>
  {title_text}
  <rect x="88" y="{subtitle_box_y}" width="{subtitle_box_width}" height="{subtitle_box_height}" rx="24" fill="#00d2ff" opacity="0.95"/>
  {subtitle_text}
</g>
<g>
  <rect x="70" y="{caption_panel_y}" width="{caption_panel_width}" height="248" rx="32" fill="#111827" opacity="0.91"/>
  <text x="108" y="{en_label_y}" font-family="Inter, Arial, sans-serif" font-size="22" font-weight="900" fill="#39ff88" letter-spacing="2">EN</text>
  {en_text}
  <text x="108" y="{zh_label_y}" font-family="Inter, Arial, sans-serif" font-size="22" font-weight="900" fill="#ffe600" letter-spacing="2">中文</text>
  {zh_text}
</g>
</svg>"##,
        accent_x = width - 130,
        accent_y = layout.top_band + 34,
        mountain_y = layout.skyline_y - 140,
        mountain_a = layout.skyline_y - 264,
        mountain_b = layout.skyline_y - 210,
        mountain_c = layout.skyline_y - 330,
        skyline_y = layout.skyline_y,
        panel_width = width - 108,
        panel_height = layout.skyline_y - 74,
        title_text = svg_text_lines(
            88,
            title_y,
            layout.title_font_size,
            layout.title_font_size + 9,
            "#101827",
            900,
            &title_lines,
        ),
        subtitle_box_y = subtitle_y - layout.subtitle_font_size,
        subtitle_box_width = width - 176,
        subtitle_box_height = (subtitle_lines.len() as i32 * (layout.subtitle_font_size + 8)) + 24,
        subtitle_text = svg_text_lines(
            108,
            subtitle_y,
            layout.subtitle_font_size,
            layout.subtitle_font_size + 8,
            "#06111f",
            800,
            &subtitle_lines,
        ),
        caption_panel_y = caption_y - 74,
        caption_panel_width = width - 140,
        en_label_y = caption_y - 24,
        en_text = svg_text_lines(
            162,
            caption_y - 24,
            layout.caption_font_size,
            layout.caption_font_size + 7,
            "#ffffff",
            700,
            &en_lines,
        ),
        zh_label_y = caption_y + 120,
        zh_text = svg_text_lines(
            162,
            caption_y + 120,
            layout.caption_font_size,
            layout.caption_font_size + 9,
            "#ffffff",
            700,
            &zh_lines,
        ),
    )
}

fn headline_from(caption: &str, max_chars: usize) -> String {
    let normalized = caption.split_whitespace().collect::<Vec<_>>().join(" ");
    let first_sentence = normalized
        .split(['.', '!', '?', '。', '！', '？'])
        .next()
        .unwrap_or(normalized.as_str())
        .trim();
    let headline = if first_sentence.is_empty() {
        normalized.as_str()
    } else {
        first_sentence
    };

    truncate_chars(headline, max_chars)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();

    if chars.next().is_some() {
        format!("{}...", truncated.trim_end())
    } else {
        truncated
    }
}

fn wrap_text(value: &str, max_chars_per_line: usize, max_lines: usize) -> Vec<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in normalized.split(' ') {
        let candidate_len =
            current.chars().count() + word.chars().count() + usize::from(!current.is_empty());
        if candidate_len > max_chars_per_line && !current.is_empty() {
            lines.push(current);
            current = String::new();

            if lines.len() == max_lines {
                if let Some(last) = lines.last_mut() {
                    *last = truncate_chars(&format!("{last} {word}"), max_chars_per_line);
                }
                break;
            }
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);

        if current.chars().count() > max_chars_per_line && lines.len() + 1 < max_lines {
            let split = truncate_chars(&current, max_chars_per_line);
            lines.push(split);
            current.clear();
        }

        if lines.len() + 1 == max_lines && current.chars().count() > max_chars_per_line {
            current = truncate_chars(&current, max_chars_per_line);
            break;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.len() > max_lines {
        lines.truncate(max_lines);
    }

    if let Some(last) = lines.last_mut() {
        *last = truncate_chars(last, max_chars_per_line);
    }

    lines
}

fn svg_text_lines(
    x: i32,
    y: i32,
    font_size: i32,
    line_height: i32,
    fill: &str,
    weight: i32,
    lines: &[String],
) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                r#"<text x="{x}" y="{}" font-family="Inter, Arial, sans-serif" font-size="{font_size}" font-weight="{weight}" fill="{fill}">{}</text>"#,
                y + (index as i32 * line_height),
                escape_xml(line)
            )
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::{headline_from, render_post_svg, wrap_text};
    use crate::drafts::{DraftStatus, PostDraft};
    use chrono::Utc;

    #[test]
    fn render_post_svg_contains_bilingual_hierarchy_and_city_visuals() {
        let draft = test_draft();
        let svg = render_post_svg(&draft);

        assert!(svg.contains("VANCOUVER"));
        assert!(svg.contains("中文"));
        assert!(svg.contains("Night market"));
        assert!(svg.contains("<rect x=\"626\""));
        assert!(svg.contains("#ff3d7f"));
    }

    #[test]
    fn headline_uses_first_sentence_and_truncates() {
        let headline = headline_from(
            "Night market returns to Richmond this Friday. Bring transit patience.",
            24,
        );

        assert_eq!(headline, "Night market returns to...");
    }

    #[test]
    fn wrap_text_limits_lines() {
        let lines = wrap_text("one two three four five six seven eight nine", 10, 2);

        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with("..."));
    }

    fn test_draft() -> PostDraft {
        PostDraft {
            id: 42,
            source_item_id: Some(7),
            caption_en: "Night market returns with food vendors, music, and late transit demand."
                .to_owned(),
            caption_zh: "夜市本週回歸，有美食攤位、音樂和更高的夜間交通需求。".to_owned(),
            status: DraftStatus::Draft,
            rendered_post_asset_ref: None,
            rendered_reel_asset_ref: None,
            created_by_sub: Some("user-1".to_owned()),
            updated_by_sub: Some("user-1".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
