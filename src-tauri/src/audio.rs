use rodio::{Decoder, OutputStream, Sink, Source};
use std::{
    fs::File,
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::mpsc,
    thread,
};
use tauri::{AppHandle, Emitter};

pub enum AudioCommand {
    Play {
        path: Option<PathBuf>,
        volume: f32,
        repeat: bool,
    },
    Stop,
}

pub fn start(app: AppHandle) -> mpsc::Sender<AudioCommand> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = OutputStream::try_default().ok();
        let mut sink: Option<Sink> = None;
        for command in receiver {
            if let Some(current) = sink.take() {
                current.stop();
            }
            if let AudioCommand::Play {
                path,
                volume,
                repeat,
            } = command
            {
                if output.is_none() {
                    output = OutputStream::try_default().ok();
                }
                let Some((_, handle)) = &output else {
                    let _ = app.emit("app-error", "无法打开声音设备，请检查系统音频输出");
                    continue;
                };
                let result = (|| -> Result<Sink, String> {
                    let next = Sink::try_new(handle).map_err(|error| error.to_string())?;
                    next.set_volume(volume.clamp(0.0, 1.0));
                    if let Some(path) = path {
                        let file = File::open(path).map_err(|error| error.to_string())?;
                        let decoder = Decoder::new(BufReader::new(file))
                            .map_err(|error| error.to_string())?;
                        if repeat {
                            next.append(decoder.repeat_infinite());
                        } else {
                            next.append(decoder);
                        }
                    } else {
                        let decoder = Decoder::new(Cursor::new(
                            include_bytes!("../assets/chime.wav").as_slice(),
                        ))
                        .map_err(|error| error.to_string())?;
                        if repeat {
                            next.append(decoder.repeat_infinite());
                        } else {
                            next.append(decoder);
                        }
                    }
                    Ok(next)
                })();
                match result {
                    Ok(next) => sink = Some(next),
                    Err(_) => {
                        let _ = app.emit("app-error", "铃声播放失败，请更换音频文件或检查声音设备");
                    }
                }
            }
        }
    });
    sender
}
