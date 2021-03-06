use std::cell::RefCell;
use std::iter::Iterator;
use std::net::{IpAddr, SocketAddr};
use std::rc::Rc;
use std::time::Duration;

use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::mpsc as mpsc_tokio;
use tracing::{error, info};

use fltk::{
    app::App,
    button::{Button, CheckButton},
    enums::Shortcut,
    frame::Frame,
    input::{Input, IntInput},
    menu::{Choice, MenuFlag},
    output::Output,
    prelude::*,
    text::{TextBuffer, TextDisplay},
    window::Window,
};

#[cfg(not(target_os = "windows"))]
use pnet::datalink;

use crate::web::{Gui2WebMessage, Web2GuiMessage};
use crate::websocket::Gui2WsMessage;

#[cfg(target_os = "linux")]
use crate::x11helper::{Capturable, X11Context};

pub fn run(log_receiver: mpsc::Receiver<String>) {
    fltk::app::lock().unwrap();
    fltk::app::unlock();
    let width = 200;
    let height = 30;
    let padding = 10;

    let app = App::default();
    let mut wind = Window::default()
        .with_size(660, 600)
        .center_screen()
        .with_label(&format!("Weylus - {}", env!("CARGO_PKG_VERSION")));

    let input_password = Input::default()
        .with_pos(200, 30)
        .with_size(width, height)
        .with_label("Password");

    let input_bind_addr = Input::default()
        .with_size(width, height)
        .below_of(&input_password, padding)
        .with_label("Bind Address");
    input_bind_addr.set_value("0.0.0.0");

    let input_port = IntInput::default()
        .with_size(width, height)
        .below_of(&input_bind_addr, padding)
        .with_label("Port");
    input_port.set_value("1701");

    let input_ws_pointer_port = IntInput::default()
        .with_size(width, height)
        .below_of(&input_port, padding)
        .with_label("Websocket Pointer Port");
    input_ws_pointer_port.set_value("9001");

    let input_ws_video_port = IntInput::default()
        .with_size(width, height)
        .below_of(&input_ws_pointer_port, padding)
        .with_label("Websocket Video Port");
    input_ws_video_port.set_value("9002");

    let input_limit_screen_updates = IntInput::default()
        .with_size(width, height)
        .below_of(&input_ws_video_port, padding)
        .with_label("Limit screen updates\n(milliseconds)");
    input_limit_screen_updates.set_value("0");

    let but_toggle = Button::default()
        .with_size(width, height)
        .below_of(&input_limit_screen_updates, 3 * padding)
        .with_label("Start");

    let mut label_enable_input = Frame::default()
        .with_pos(430, 30)
        .with_size(width, 15)
        .with_label("Enabled input methods:");
    label_enable_input.set_tooltip(
        "Specifies which types of pointerevents from the browser will \
        be accepted. This might be useful if touch rejection does not work properly and you only \
        want to use a pen/stylus.",
    );

    let check_enable_mouse = CheckButton::default()
        .with_size(64, height)
        .below_of(&label_enable_input, 0)
        .with_label("Mouse");
    check_enable_mouse.set_checked(true);

    let check_enable_stylus = CheckButton::default()
        .with_size(64, height)
        .right_of(&check_enable_mouse, 2)
        .with_label("Stylus");
    check_enable_stylus.set_checked(true);

    let check_enable_touch = CheckButton::default()
        .with_size(63, height)
        .right_of(&check_enable_stylus, 2)
        .with_label("Touch");
    check_enable_touch.set_checked(true);

    let mut label_only_linux = Frame::default()
        .with_size(width, 15)
        .below_of(&check_enable_mouse, 5)
        .with_label("Available only on Linux:");
    #[cfg(target_os = "linux")]
    label_only_linux.hide();

    #[allow(unused_mut)]
    let mut check_stylus = CheckButton::default()
        .with_pos(430, padding + 3 * height)
        .with_size(width, height)
        .with_label("Stylus && Touch Simulation");
    check_stylus.set_tooltip(
        "Enables things like pressure sensitivity and multitouch. \
        Requires /dev/uinput to be writable!",
    );
    #[cfg(target_os = "linux")]
    {
        check_stylus.set_checked(true);
    }
    #[cfg(not(target_os = "linux"))]
    {
        check_stylus.deactivate();
    }

    let mut check_faster_screencapture = CheckButton::default()
        .with_size(width, height)
        .below_of(&check_stylus, padding)
        .with_label("Better screen capturing");

    check_faster_screencapture.set_tooltip(
        "Enables faster screen capturing and more fine grained \
        control about what to capture.",
    );

    #[allow(unused_mut)]
    let mut check_capture_cursor = CheckButton::default()
        .with_size(width, height)
        .below_of(&check_faster_screencapture, padding)
        .with_label("Capture Cursor");

    #[cfg(target_os = "linux")]
    {
        check_capture_cursor.set_checked(false);
        check_faster_screencapture.set_checked(true);
    }
    #[cfg(not(target_os = "linux"))]
    {
        check_faster_screencapture.deactivate();
        check_capture_cursor.deactivate();
    }

    let label_capturable_choice = Frame::default()
        .with_size(width, height)
        .below_of(&check_capture_cursor, padding)
        .with_label("Capture:");

    #[allow(unused_mut)]
    let mut choice_capturable = Choice::default()
        .with_size(width, height)
        .below_of(&label_capturable_choice, 0);
    #[cfg(not(target_os = "linux"))]
    choice_capturable.deactivate();

    let mut but_update_capturables = Button::default()
        .with_size(width, height)
        .below_of(&choice_capturable, padding)
        .with_label("Refresh");
    but_update_capturables.set_tooltip(
        "Refresh list of capturable objects, e. g. if you opened a \
        new window after starting Weylus.",
    );
    #[cfg(not(target_os = "linux"))]
    but_update_capturables.deactivate();

    let output_buf = TextBuffer::default();
    let output = TextDisplay::default(output_buf)
        .with_size(600, 6 * height)
        .with_pos(30, 600 - 30 - 6 * height);

    let mut output_server_addr = Output::default()
        .with_size(500, height)
        .with_pos(130, 600 - 30 - 7 * height - 3 * padding)
        .with_label("Connect your\ntablet to:");
    output_server_addr.hide();

    let mut but_show_qr = Button::default()
        .with_size(120, height)
        .with_pos(but_toggle.x() - 165, but_toggle.y())
        .with_label("Show QR Code");

    but_show_qr.hide();

    wind.make_resizable(true);
    wind.end();
    wind.show();

    let wind_ref = Rc::new(RefCell::new(wind));

    let but_toggle_ref = Rc::new(RefCell::new(but_toggle));
    let but_update_capturables_ref = Rc::new(RefCell::new(but_update_capturables));
    let choice_capturable_ref = Rc::new(RefCell::new(choice_capturable));
    let check_faster_screencapture_ref = Rc::new(RefCell::new(check_faster_screencapture));
    let check_capture_cursor_ref = Rc::new(RefCell::new(check_capture_cursor));
    let output_server_addr = Arc::new(Mutex::new(output_server_addr));
    let output = Arc::new(Mutex::new(output));

    let qr_popup_ref = Rc::new(RefCell::new(Window::default()));
    let qr_img_frame_ref = Rc::new(RefCell::new(Frame::new(0, 0, 0, 0, "")));
    qr_popup_ref.borrow().end();

    let (sender_ws2gui, _receiver_ws2gui) = mpsc::channel();
    let (sender_web2gui, receiver_web2gui) = mpsc::channel();

    std::thread::spawn(move || {
        while let Ok(log_message) = log_receiver.recv() {
            let output = output.lock().unwrap();
            output.insert(&log_message);
        }
    });

    {
        let output_server_addr = output_server_addr.clone();
        std::thread::spawn(move || {
            while let Ok(message) = receiver_web2gui.recv() {
                match message {
                    Web2GuiMessage::Shutdown => {
                        let mut output_server_addr = output_server_addr.lock().unwrap();
                        output_server_addr.hide();
                    }
                }
            }
        });
    }

    #[cfg(target_os = "linux")]
    let mut x11_context = X11Context::new().unwrap();
    #[cfg(target_os = "linux")]
    let current_capturable = Rc::new(RefCell::new(Option::<Capturable>::None));

    #[cfg(target_os = "linux")]
    {
        let current_capturable = current_capturable.clone();

        {
            let choice_capturable_ref = choice_capturable_ref.clone();
            but_update_capturables_ref
                .borrow_mut()
                .set_callback(Box::new(move || {
                    let mut choice_capturable = choice_capturable_ref.borrow_mut();
                    choice_capturable.clear();
                    let capturables = x11_context.capturables().unwrap();
                    {
                        let mut current_capturable = current_capturable.borrow_mut();
                        if current_capturable.is_none() {
                            let first_capturable = capturables[0].clone();
                            current_capturable.replace(first_capturable);
                        }
                    }
                    for c in capturables {
                        let current_capturable = current_capturable.clone();
                        let chars = c
                            .name()
                            .replace("\\", "\\\\")
                            .replace("/", "\\/")
                            .replace("_", "\\_")
                            .replace("&", "\\&");
                        let chars = chars.chars();
                        let mut name = String::new();
                        for (i, c) in chars.enumerate() {
                            if i >= 32 {
                                name.push_str("...");
                                break;
                            }
                            name.push(c);
                        }
                        choice_capturable.add(
                            &name,
                            Shortcut::None,
                            MenuFlag::Normal,
                            Box::new(move || {
                                current_capturable.replace(Some(c.clone()));
                            }),
                        );
                    }
                }));
        }

        but_update_capturables_ref.borrow_mut().do_callback();

        let check_faster_screencapture_ref = check_faster_screencapture_ref.clone();
        let check_capture_cursor_ref = check_capture_cursor_ref.clone();
        let but_update_capturables_ref = but_update_capturables_ref.clone();

        check_faster_screencapture_ref
            .clone()
            .borrow_mut()
            .set_callback(Box::new(move || {
                let checked = !check_faster_screencapture_ref.borrow().is_checked();
                let mut choice_capturable = choice_capturable_ref.borrow_mut();
                if checked {
                    choice_capturable.deactivate();
                    but_update_capturables_ref.borrow_mut().deactivate();
                    check_capture_cursor_ref.borrow_mut().deactivate();
                } else {
                    choice_capturable.activate();
                    but_update_capturables_ref.borrow_mut().activate();
                    check_capture_cursor_ref.borrow_mut().activate();
                }
            }));
    }

    let mut sender_gui2ws: Option<mpsc::Sender<Gui2WsMessage>> = None;
    let mut sender_gui2web: Option<mpsc_tokio::Sender<Gui2WebMessage>> = None;

    let mut is_server_running = false;

    let but_toggle_ref2 = but_toggle_ref.clone();
    let wind_ref2 = wind_ref.clone();

    but_toggle_ref
        .clone()
        .borrow_mut()
        .set_callback(Box::new(move || {
            if let Err(err) = || -> Result<(), Box<dyn std::error::Error>> {
                let but_toggle_ref = but_toggle_ref.clone();
                let mut but = but_toggle_ref.try_borrow_mut()?;

                let wind_ref = wind_ref.clone();
                let qr_popup_ref = qr_popup_ref.clone();
                let qr_img_frame_ref = qr_img_frame_ref.clone();

                if !is_server_running {
                    let password_string = input_password.value();
                    let password = match password_string.as_str() {
                        "" => None,
                        pw => Some(pw),
                    };
                    let bind_addr: IpAddr = input_bind_addr.value().parse()?;
                    let web_port: u16 = input_port.value().parse()?;
                    let ws_pointer_port: u16 = input_ws_pointer_port.value().parse()?;
                    let ws_video_port: u16 = input_ws_video_port.value().parse()?;
                    let screen_update_interval: u64 = input_limit_screen_updates.value().parse()?;
                    let screen_update_interval = Duration::from_millis(screen_update_interval);

                    let (sender_gui2ws_tmp, receiver_gui2ws) = mpsc::channel();
                    sender_gui2ws = Some(sender_gui2ws_tmp);
                    #[cfg(target_os = "linux")]
                    {
                        let faster_screencapture =
                            check_faster_screencapture_ref.borrow().is_checked();
                        if !faster_screencapture {
                            current_capturable.replace(None);
                            but_update_capturables_ref.borrow_mut().do_callback();
                        }
                        crate::websocket::run(
                            sender_ws2gui.clone(),
                            receiver_gui2ws,
                            SocketAddr::new(bind_addr, ws_pointer_port),
                            SocketAddr::new(bind_addr, ws_video_port),
                            password,
                            screen_update_interval,
                            check_stylus.is_checked(),
                            faster_screencapture,
                            current_capturable
                                .clone()
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .clone(),
                            check_capture_cursor_ref.borrow().is_checked(),
                            check_enable_mouse.is_checked(),
                            check_enable_stylus.is_checked(),
                            check_enable_touch.is_checked(),
                        );
                    }
                    #[cfg(not(target_os = "linux"))]
                    crate::websocket::run(
                        sender_ws2gui.clone(),
                        receiver_gui2ws,
                        SocketAddr::new(bind_addr, ws_pointer_port),
                        SocketAddr::new(bind_addr, ws_video_port),
                        password,
                        screen_update_interval,
                        check_enable_mouse.is_checked(),
                        check_enable_stylus.is_checked(),
                        check_enable_touch.is_checked(),
                    );

                    let (sender_gui2web_tmp, receiver_gui2web) = mpsc_tokio::channel(100);
                    sender_gui2web = Some(sender_gui2web_tmp);
                    let mut web_sock = SocketAddr::new(bind_addr, web_port);
                    crate::web::run(
                        sender_web2gui.clone(),
                        receiver_gui2web,
                        &web_sock,
                        ws_pointer_port,
                        ws_video_port,
                        password,
                    );

                    #[cfg(not(target_os = "windows"))]
                    {
                        if web_sock.ip().is_unspecified() {
                            // try to guess an ip
                            let mut ips = Vec::<IpAddr>::new();
                            for iface in datalink::interfaces()
                                .iter()
                                .filter(|iface| iface.is_up() && !iface.is_loopback())
                            {
                                for ipnetw in &iface.ips {
                                    if (ipnetw.is_ipv4() && web_sock.ip().is_ipv4())
                                        || (ipnetw.is_ipv6() && web_sock.ip().is_ipv6())
                                    {
                                        // filtering ipv6 unicast requires nightly or more fiddling,
                                        // lets wait for nightlies to stabilize...
                                        ips.push(ipnetw.ip())
                                    }
                                }
                            }
                            if !ips.is_empty() {
                                web_sock.set_ip(ips[0]);
                            }
                            if ips.len() > 1 {
                                info!("Found more than one IP address for browsers to connect to,");
                                info!("other urls are:");
                                for ip in &ips[1..] {
                                    info!("http://{}", SocketAddr::new(*ip, web_port));
                                }
                            }
                        }
                    }
                    let mut output_server_addr = output_server_addr.lock()?;

                    #[cfg(not(target_os = "windows"))]
                    {
                        use image::Luma;
                        use qrcode::QrCode;
                        let addr_string = format!("http://{}", web_sock.to_string());
                        output_server_addr.set_value(&addr_string);
                        let password = password.map(|pw| pw.to_string());
                        but_show_qr.set_callback(Box::new(move || {
                            let mut url_string = addr_string.clone();
                            if let Some(password) = &password {
                                url_string.push_str("?password=");
                                url_string.push_str(
                                    &percent_encoding::utf8_percent_encode(
                                        &password,
                                        percent_encoding::NON_ALPHANUMERIC,
                                    )
                                    .to_string(),
                                );
                                info!("{}", &url_string);
                            }
                            let code = QrCode::new(&url_string).unwrap();
                            let img_buf = code.render::<Luma<u8>>().build();
                            let width = img_buf.width() as i32;
                            let height = img_buf.height() as i32;
                            let image = image::DynamicImage::ImageLuma8(img_buf);
                            let mut buf = vec![];
                            image
                                .write_to(&mut buf, image::ImageOutputFormat::Png)
                                .unwrap();
                            let png = fltk::image::PngImage::from_data(&buf).unwrap();

                            let mut qr_popup = qr_popup_ref.borrow_mut();
                            let wind = wind_ref.borrow();
                            qr_popup.resize(
                                wind.x() + (wind.width() - width) / 2,
                                wind.y() + (wind.height() - height) / 2,
                                width,
                                height,
                            );
                            qr_popup.set_label(&format!(
                                "Weylus - QR Code for: {}",
                                web_sock.to_string(),
                            ));
                            let mut qr_img_frame = qr_img_frame_ref.borrow_mut();
                            qr_img_frame.resize(0, 0, width, height);
                            qr_img_frame.set_image(&png);
                            qr_popup.show();
                            qr_popup.make_current();
                        }));
                        but_show_qr.show();
                    }
                    #[cfg(target_os = "windows")]
                    {
                        if web_sock.ip().is_unspecified() {
                            output_server_addr.set_value("http://<your ip address>");
                        } else {
                            output_server_addr
                                .set_value(&format!("http://{}", web_sock.to_string()));
                        }
                    }
                    output_server_addr.show();
                    but.set_label("Stop");
                } else {
                    if let Some(mut sender_gui2web) = sender_gui2web.clone() {
                        sender_gui2web.try_send(Gui2WebMessage::Shutdown)?;
                    }

                    if let Some(sender_gui2ws) = sender_gui2ws.clone() {
                        sender_gui2ws.send(Gui2WsMessage::Shutdown)?;
                    }
                    but.set_label("Start");
                    but_show_qr.hide();
                    qr_popup_ref.borrow_mut().hide();
                }
                is_server_running = !is_server_running;
                Ok(())
            }() {
                error!("{}", err);
            };
        }));

    wind_ref2.borrow_mut().handle(Box::new(move |ev| match ev {
        fltk::Event::Hide => {
            if is_server_running {
                but_toggle_ref2.borrow_mut().do_callback();
            }
            std::process::exit(0);
        }
        _ => false,
    }));

    app.run().expect("Failed to run Gui!");
}
