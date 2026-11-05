//! The node: the one in use with Rust's checks, others to change to, each
//! checked, and adding one, which has to be checked before it can be used.

use std::sync::Arc;

use fob_app::{
    Engine,
    views::{CheckMark, NodeCheckObserver, NodeChoice, NodeReport, NodeSetting, NodeUse},
};
use relm4::{
    Sender,
    adw::{self, prelude::*},
    gtk,
    prelude::*,
};

use crate::{bridge::View, root::Msg as RootMsg, widgets};

#[derive(Debug)]
pub enum Input {
    Opened,
    Report(u64, View<NodeReport>),
    Choices(u64, View<Vec<NodeChoice>>),
    Use(String, bool),
    Used,
    Remove(String),
    Edited,
    Check,
    Checked(u64, View<NodeReport>),
    AddChecked,
    Failed(String),
}

pub struct Node {
    engine: Engine,
    /// Counts rounds of checks: what a stale one hears is dropped.
    round: u64,
    adding_round: u64,
    changing: bool,
    checking_new: bool,
    new_report: Option<View<NodeReport>>,
    error: Option<String>,
    current: gtk::Box,
    others: gtk::ListBox,
    url: adw::EntryRow,
    trusted: adw::SwitchRow,
    new_rows: gtk::Box,
}

struct Observer {
    to: Sender<Input>,
    wrap: Box<dyn Fn(View<NodeReport>) -> Input + Send + Sync>,
}

impl NodeCheckObserver for Observer {
    fn on_progress(&self, report: NodeReport) {
        let _ = self.to.send((self.wrap)(View(report.into())));
    }
}

fn mark_icon(mark: &CheckMark) -> gtk::Widget {
    let icon = match mark {
        CheckMark::Pass => "emblem-ok-symbolic",
        CheckMark::Fail => "dialog-warning-symbolic",
        CheckMark::Checking => return gtk::Spinner::builder().spinning(true).build().upcast(),
        CheckMark::Skipped => "list-remove-symbolic",
    };
    let image = gtk::Image::from_icon_name(icon);
    match mark {
        CheckMark::Pass => image.add_css_class("success"),
        CheckMark::Fail => image.add_css_class("error"),
        _ => image.add_css_class("dim-label"),
    }
    image.upcast()
}

fn check_rows(parent: &gtk::Box, report: &NodeReport) {
    let list = widgets::list();
    for check in &report.rows {
        let line = match &check.second {
            Some(second) => format!("{}\n{second}", check.result),
            None => check.result.clone(),
        };
        let row = widgets::row(&check.title, &line);
        row.add_prefix(&mark_icon(&check.mark));
        row.upcast_ref::<gtk::Widget>().update_property(&[gtk::accessible::Property::Label(&check.label)]);
        list.append(&row);
    }
    parent.append(&list);
}

#[relm4::component(pub)]
impl SimpleComponent for Node {
    type Init = Engine;
    type Input = Input;
    type Output = RootMsg;

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                pack_end = &gtk::Button {
                    set_icon_name: "view-refresh-symbolic",
                    set_tooltip_text: Some("Check again"),
                    connect_clicked => Input::Opened,
                },
            },
            #[wrap(Some)]
            set_content = &gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                #[wrap(Some)]
                set_child = &adw::Clamp {
                    set_maximum_size: 600,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 16,
                        #[local_ref]
                        current -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                        },
                        gtk::Label {
                            add_css_class: "error",
                            set_wrap: true,
                            #[watch]
                            set_visible: model.error.is_some(),
                            #[watch]
                            set_label: model.error.as_deref().unwrap_or(""),
                        },
                        adw::PreferencesGroup {
                            set_title: "Other nodes",
                            #[local_ref]
                            add = others -> gtk::ListBox {
                                add_css_class: "boxed-list",
                                set_selection_mode: gtk::SelectionMode::None,
                            },
                        },
                        adw::PreferencesGroup {
                            set_title: "Add a node",
                            #[local_ref]
                            add = url -> adw::EntryRow {
                                set_title: "Node address",
                                connect_changed => Input::Edited,
                            },
                            #[local_ref]
                            add = trusted -> adw::SwitchRow {
                                set_title: "Accept its self-signed certificate",
                                connect_active_notify => Input::Edited,
                            },
                        },
                        #[local_ref]
                        new_rows -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                        },
                        gtk::Box {
                            set_halign: gtk::Align::Center,
                            set_spacing: 12,
                            gtk::Button {
                                add_css_class: "pill",
                                #[watch]
                                set_label: if model.checking_new { "Checking…" } else if model.new_report.is_some() { "Check again" } else { "Check" },
                                #[watch]
                                set_sensitive: !model.checking_new,
                                connect_clicked => Input::Check,
                            },
                            gtk::Button {
                                add_css_class: "pill",
                                add_css_class: "suggested-action",
                                #[watch]
                                set_visible: model.new_use().is_some(),
                                #[watch]
                                set_label: model.new_use().unwrap_or("Use"),
                                #[watch]
                                set_sensitive: !model.changing,
                                connect_clicked => Input::AddChecked,
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(engine: Engine, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let (current, new_rows) = (gtk::Box::new(gtk::Orientation::Vertical, 8), gtk::Box::new(gtk::Orientation::Vertical, 8));
        let (others, url, trusted) = (gtk::ListBox::new(), adw::EntryRow::new(), adw::SwitchRow::new());
        let model = Node {
            engine,
            round: 0,
            adding_round: 0,
            changing: false,
            checking_new: false,
            new_report: None,
            error: None,
            current: current.clone(),
            others: others.clone(),
            url: url.clone(),
            trusted: trusted.clone(),
            new_rows: new_rows.clone(),
        };
        let (current, new_rows, others, url, trusted) = (&current, &new_rows, &others, &url, &trusted);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        match input {
            Input::Opened => self.check(&sender),
            Input::Report(round, View(report)) => {
                if round == self.round {
                    self.draw_current(&report);
                }
            }
            Input::Choices(round, View(choices)) => {
                if round == self.round {
                    self.draw_others(&choices, &sender);
                }
            }
            Input::Use(url, self_signed) => {
                if self.changing {
                    return;
                }
                self.changing = true;
                self.error = None;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(move |wallet| async move { wallet.set_node(url, self_signed).await }).await {
                        Ok(()) => sender.input(Input::Used),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
            }
            Input::Used => {
                self.changing = false;
                self.check(&sender);
            }
            Input::Remove(url) => {
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    match engine.change(move |wallet| async move { wallet.remove_node(url).await }).await {
                        Ok(()) => sender.input(Input::Used),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
            }
            // Another address or box makes the last checks stale.
            Input::Edited => {
                self.adding_round += 1;
                self.checking_new = false;
                self.new_report = None;
                widgets::empty(&self.new_rows);
            }
            Input::Check => self.check_new(&sender),
            Input::Checked(round, report) => {
                if round == self.adding_round {
                    widgets::empty(&self.new_rows);
                    self.new_rows.append(&widgets::words(&report.0.verdict));
                    check_rows(&self.new_rows, &report.0);
                    self.checking_new = report.0.checking;
                    self.new_report = Some(report);
                }
            }
            Input::AddChecked => {
                let Some(View(report)) = &self.new_report else { return };
                let (url, self_signed) = (report.node.url.clone(), report.node.self_signed);
                self.changing = true;
                let (engine, sender) = (self.engine.clone(), sender.clone());
                relm4::spawn(async move {
                    let (given, added) = (url.clone(), url);
                    let done = engine
                        .change(move |wallet| async move {
                            wallet.add_node(NodeSetting { url: added, self_signed }).await?;
                            wallet.set_node(given, self_signed).await
                        })
                        .await;
                    match done {
                        Ok(()) => sender.input(Input::Used),
                        Err(e) => sender.input(Input::Failed(e.to_string())),
                    }
                });
                self.url.set_text("");
            }
            Input::Failed(why) => {
                self.changing = false;
                self.error = Some(why);
            }
        }
    }
}

impl Node {
    /// What adding says once the new node's checks allow it.
    fn new_use(&self) -> Option<&'static str> {
        let View(report) = self.new_report.as_ref()?;
        match (report.checking, &report.usable) {
            (true, _) | (_, NodeUse::No) => None,
            (_, NodeUse::Use) => Some("Use"),
            (_, NodeUse::UseAnyway) => Some("Use it anyway"),
        }
    }

    /// The node in use, then the others, each as it finishes.
    fn check(&mut self, sender: &ComponentSender<Self>) {
        self.round += 1;
        let round = self.round;
        let (engine, to) = (self.engine.clone(), sender.input_sender().clone());
        let then = sender.clone();
        relm4::spawn(async move {
            let observer: Arc<dyn NodeCheckObserver> = Arc::new(Observer { to, wrap: Box::new(move |report| Input::Report(round, report)) });
            let report = engine
                .read(move |wallet, _| async move {
                    let node = wallet.node().await;
                    wallet.check_node(node, observer).await
                })
                .await;
            if let Some(report) = report {
                then.input(Input::Report(round, View(report.into())));
            }
            let choices = engine
                .read(|wallet, _| async move {
                    let others = wallet.other_nodes().await;
                    wallet.check_nodes(others.into_iter().map(|o| o.node).collect()).await
                })
                .await;
            if let Some(choices) = choices {
                then.input(Input::Choices(round, View(choices.into())));
            }
        });
    }

    fn check_new(&mut self, sender: &ComponentSender<Self>) {
        let url = self.url.text().trim().to_owned();
        if url.is_empty() {
            return;
        }
        self.adding_round += 1;
        self.checking_new = true;
        let round = self.adding_round;
        let self_signed = self.trusted.is_active();
        let (engine, to, then) = (self.engine.clone(), sender.input_sender().clone(), sender.clone());
        relm4::spawn(async move {
            let observer: Arc<dyn NodeCheckObserver> = Arc::new(Observer { to, wrap: Box::new(move |report| Input::Checked(round, report)) });
            let report = engine.read(move |wallet, _| async move { wallet.check_node(NodeSetting { url, self_signed }, observer).await }).await;
            if let Some(report) = report {
                then.input(Input::Checked(round, View(report.into())));
            }
        });
    }

    fn draw_current(&self, report: &NodeReport) {
        widgets::empty(&self.current);
        self.current.append(&widgets::heading(&report.verdict));
        let address = widgets::centred(&report.address);
        address.set_selectable(true);
        self.current.append(&address);
        check_rows(&self.current, report);
    }

    fn draw_others(&self, choices: &[NodeChoice], sender: &ComponentSender<Self>) {
        widgets::clear(&self.others);
        for choice in choices {
            let row = widgets::row(&choice.address, &choice.line);
            row.add_prefix(&mark_icon(&choice.mark));
            if choice.removable {
                let remove = gtk::Button::builder().icon_name("user-trash-symbolic").valign(gtk::Align::Center).tooltip_text("Remove").css_classes(["flat"]).build();
                let (url, then) = (choice.node.url.clone(), sender.clone());
                remove.connect_clicked(move |_| then.input(Input::Remove(url.clone())));
                row.add_suffix(&remove);
            }
            if choice.can_use {
                let pick = gtk::Button::builder().label("Use").valign(gtk::Align::Center).build();
                let (url, self_signed, then) = (choice.node.url.clone(), choice.node.self_signed, sender.clone());
                pick.connect_clicked(move |_| then.input(Input::Use(url.clone(), self_signed)));
                row.add_suffix(&pick);
            }
            self.others.append(&row);
        }
    }
}
