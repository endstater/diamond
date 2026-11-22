use std::path::PathBuf;
use std::sync::mpsc::{self,Receiver};

pub enum Choice {
    Folder(PathBuf),
    File(PathBuf),
    Cancelled,
    Unavailable(String),
}

pub fn choose(folder:bool,ctx:egui::Context)->Receiver<Choice> {
    let (sender,receiver)=mpsc::channel();
    std::thread::spawn(move || {
        let result=pollster::block_on(async {
            let request=ashpd::desktop::file_chooser::SelectedFiles::open_file()
                .title(if folder {"Открыть хранилище"} else {"Открыть манифест плагина"})
                .directory(folder).multiple(false).send().await?;
            request.response()
        });
        let choice=match result {
            Ok(files) => match files.uris().first().and_then(|url|url.to_file_path().ok()) {
                Some(path) if folder => Choice::Folder(path),
                Some(path) => Choice::File(path),
                None => Choice::Cancelled,
            },
            Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => Choice::Cancelled,
            Err(error) => Choice::Unavailable(error.to_string()),
        };
        let _=sender.send(choice);ctx.request_repaint();
    });
    receiver
}
