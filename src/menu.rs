use kas::prelude::*;
use kas::widgets::menu::MenuBar;

use super::GlobalMsg;

#[derive(Clone, Debug)]
enum Msg {}

impl_scope! {
  #[widget{
    layout = column: [
      align(left): self.display,
    ];
  }]
  #[derive(Debug)]
  pub struct Menu {
    core: widget_core!(),
    #[widget] display: MenuBar,
  }
  impl Self {
    pub fn new() -> Self {
      Menu {
        core: Default::default(),
        display: MenuBar::<kas::dir::Right>::builder()
          .menu("&File", |menu| {
            menu.entry("New &Movie", GlobalMsg::TryLoadMovie);
          })
          .build()
      }
    }
  }
  impl Widget for Self {
    fn handle_message(&mut self, mgr: &mut EventMgr) {
      if let Some(_msg) = mgr.try_pop::<Msg>() {
      }
    }
  }
}
