//! Following links: external URLs, in-document anchors, attachments and other pages.
use super::{Event, message};
use crate::{App, Message, services::blocking};
use iced::{Task, widget};
use norito_model::text::decode_fragment;
use norito_platform::{FileRef, Result, is_markdown};
use norito_viewmodel::{Action, Command};

const SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// Where a link destination leads. Anchors are percent-decoded.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Target {
    External,
    Anchor(String),
    Attachment,
    Page(String),
}

pub(super) fn classify(destination: &str) -> Target {
    if destination.split_once(':').is_some_and(|(scheme, _)| {
        SCHEMES
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    }) {
        return Target::External;
    }
    let (path, anchor) = destination.split_once('#').unwrap_or((destination, ""));
    let anchor = decode_fragment(anchor);
    if path.is_empty() {
        Target::Anchor(anchor)
    } else if is_markdown(path) {
        Target::Page(anchor)
    } else {
        Target::Attachment
    }
}

impl App {
    pub(super) fn follow(&mut self, destination: String) -> Task<Message> {
        let target = classify(&destination);
        let opener = self.services.opener.clone();
        if target == Target::External {
            return blocking(
                move || opener.open_external(&destination),
                |result| message(Event::ExternalOpened(result)),
            );
        }
        if let Target::Anchor(anchor) = &target {
            let offset = anchor
                .strip_prefix("fn:")
                .and_then(|name| self.features.structure.footnote(name))
                .or_else(|| {
                    self.features
                        .structure
                        .heading(anchor)
                        .map(|heading| heading.offset)
                });
            let Some(offset) = offset else {
                self.vm.status.fail("見出しが見つかりません。");
                return Task::none();
            };
            self.editor.jump(offset);
            return widget::operation::focus("document");
        }
        let Some(file) = self.vm.session.handle() else {
            self.vm
                .status
                .fail("ページ間リンクを開くには文書を保存してください。");
            return Task::none();
        };
        match target {
            Target::Page(anchor) => {
                let resources = self.services.resources.clone();
                let epoch = self.vm.session.epoch;
                blocking(
                    move || resources.resolve_page(file, &destination),
                    move |result| message(Event::PageReady(result, anchor.clone(), epoch)),
                )
            }
            _ => blocking(
                move || opener.open_attachment(file, &destination),
                |result| message(Event::ExternalOpened(result)),
            ),
        }
    }
    /// Open a resolved page through the unsaved check, then jump to `anchor`.
    pub(super) fn page_ready(
        &mut self,
        result: Result<FileRef>,
        anchor: String,
        epoch: u64,
    ) -> Task<Message> {
        if epoch != self.vm.session.epoch || self.vm.busy() || self.vm.pending.is_some() {
            return Task::none();
        }
        match result {
            Ok(file) => {
                let task = self.dispatch(Command::Navigate(Action::OpenFile(file)), false);
                self.features.pending_anchor = Some(anchor);
                task
            }
            Err(e) => {
                self.vm.status.fail(e.message);
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destinations_classify_by_scheme_path_and_extension() {
        assert_eq!(classify("HTTPS://example.com"), Target::External);
        assert_eq!(
            classify("#%E8%A6%8B%E5%87%BA%E3%81%97"),
            Target::Anchor("見出し".into())
        );
        assert_eq!(classify("#fn:1"), Target::Anchor("fn:1".into()));
        assert_eq!(classify("other.MD#a"), Target::Page("a".into()));
        assert_eq!(classify("assets/file.pdf"), Target::Attachment);
        assert_eq!(classify("javascript:alert(1)"), Target::Attachment);
    }
}
