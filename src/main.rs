use clap::{Parser, Args as OtherArgs};
use core::fmt;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const TICK_FOR_OPERATION_TIMEOUT: u64 = 10;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, author = "MK", hide = false)]
#[group(id="direction", multiple = false, required = true)]
struct Args {
    #[command(flatten)]
    direct: GroupTry,

    /// specifies port for UDP communication
    #[arg(short, long, requires = "direction")]
    port: u16,

    /// specifies IP address for UDP communication
    #[arg(short, long, requires = "direction")]
    ip_address: String,

    /// specifies timeout for receive thread
    #[arg(short, long, requires = "direction")]
    timeout: Option<u32>,
}

#[derive(OtherArgs, Debug)]
struct GroupTry {
    /// specifies if send thread needs to be started
    #[arg(short, long, group = "direction")]
    send: bool,

    /// specifies if receive thread needs to be started
    #[arg(short, long, group = "direction")]
    receive: bool,
}

#[derive(Debug, PartialEq)]
enum Direction {
    None,
    Send,
    Receive,
    SendReceive,
}

struct App {
    ip_addr: SocketAddr,
    dir: Direction,
    receive_timeout: u32,
}

impl From<SocketAddr> for App {
    fn from(ip_addr: SocketAddr) -> Self {
        App {
            ip_addr: ip_addr,
            dir: Direction::SendReceive,
            receive_timeout: 10,
        }
    }
}

impl std::fmt::Display for App {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Timeout: {} \nDirection: {} \n", self.receive_timeout, {
            match self.dir {
                Direction::None => "No Transmit nor Receive".to_string(),
                Direction::Receive => "Receiving....".to_string(),
                Direction::Send => "Transmitting....".to_string(),
                Direction::SendReceive => "Receiving and Transmitting....".to_string(),
            }
        })
    }
}

impl App {
    fn new() -> App {
        App {
            dir: Direction::None,
            ip_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 62345),
            receive_timeout: 10,
        }
    }

    fn transmit_thread(
        msg_to_me: &std::sync::mpsc::Receiver<String>,
        to_main: &std::sync::mpsc::SyncSender<String>,
        timeout: u32,
        ip_address: SocketAddr,
    ) {
        let start = Instant::now();
        let time_out = Duration::from_secs(timeout as u64);
        let mut count = 0;

        println!("Transmit thread started.");

        let target: SocketAddr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 101)), 22223);
        let socket = match UdpSocket::bind(ip_address) {
            Ok(sock) => sock,
            Err(_) => {
                println!("Cannot bind to the socket");
                println!("Time spent in thread: {:?}", start.elapsed());
                to_main.send("quiting".to_string()).unwrap();
                return;
            }
        };
        let _ = socket.connect(target);
        // socket.set_broadcast(true).expect("Failed to set broadcast");
        'klop: loop {
            match msg_to_me.recv_timeout(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT)) {
                Ok(message) => {
                    if message.contains("quit") {
                        break;
                    }
                }
                _ => {}
            }

            if timeout != 0 && start.elapsed() >= time_out {
                break 'klop;
            }

            let mut message: Vec<u8> = (format!("$SCP,0,1,{}", count)).into_bytes().to_vec();
            println!("Sedning message: {:?}", message);
            count += 1;
            match socket.send_to(&mut message, ip_address) {
                Ok(_) => {}
                Err(e) => println!("packtet {} dropped with error: {:?}", count - 1, e),
            }
            sleep(Duration::from_secs(1));
        }

        println!("Time spent in thread: {:?}", start.elapsed());
        to_main.send("quiting".to_string()).unwrap();
    }

    fn receive_thread(
        msg_to_me: &std::sync::mpsc::Receiver<String>,
        to_main: &std::sync::mpsc::SyncSender<String>,
        timeout: u32,
        ip_address: SocketAddr,
    ) {
        let start = Instant::now();
        let time_out = Duration::from_secs(timeout as u64);

        println!("Receive thread started.");

        let socket: UdpSocket;
        println!("Socket address: {:?}", ip_address);

        match UdpSocket::bind(ip_address) {
            Ok(sock) => socket = sock,
            Err(_) => {
                println!("Cannot bind to the socket");
                println!("Time spent in thread: {:?}", start.elapsed());
                to_main.send("quiting".to_string()).unwrap();
                return;
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT)))
            .unwrap();

        let mut rx_buf = vec![0; 2048];
        let mut pause: bool = false;

        'klop: loop {
            match msg_to_me.recv_timeout(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT * 10)) {
                Ok(message) => {
                    if message.contains("quit") {
                        break;
                    }
                    if message.contains("pause") {
                        pause = true;
                    }
                    if message.contains("restore") {
                        pause = false;
                    }
                }
                _ => {}
            }

            match socket.recv(&mut rx_buf) {
                Ok(size) => {
                    if size != 0 {
                        if pause == false {
                            print!("{}", String::from_utf8_lossy(&rx_buf));
                        }
                        rx_buf.fill(0);
                    }
                }
                Err(_) => {}
            }
            if timeout != 0 && start.elapsed() >= time_out {
                break 'klop;
            }
        }

        println!("Time spent in thread: {:?}", start.elapsed());
        to_main.send("quiting".to_string()).unwrap();
    }

    fn run(self) {
        println!("{}", self);

        let receive_handle: std::thread::JoinHandle<()>;
        let transmit_handle: std::thread::JoinHandle<()>;
        let (to_receiver, for_receiver) = mpsc::sync_channel::<String>(100);
        let (form_threads, to_main) = mpsc::sync_channel::<String>(100);
        let (to_transmiter, for_transmiter) = mpsc::sync_channel::<String>(100);

        let form_threads2 = form_threads.clone();

        if self.dir == Direction::Receive || self.dir == Direction::SendReceive {
            println!("Starting receiving thread ....");
            receive_handle = std::thread::spawn(move || {
                Self::receive_thread(
                    &for_receiver,
                    &form_threads,
                    self.receive_timeout,
                    self.ip_addr,
                )
            });
        } else {
            receive_handle = std::thread::spawn(move || {});
        }

        if self.dir == Direction::Send || self.dir == Direction::SendReceive {
            println!("Starting transmiting thread ....");
            transmit_handle = std::thread::spawn(move || {
                Self::transmit_thread(
                    &for_transmiter,
                    &form_threads2,
                    self.receive_timeout,
                    self.ip_addr,
                )
            });
        } else {
            transmit_handle = std::thread::spawn(move || {});
        }

        'ThreadLoop: loop {
            if event::poll(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT)).unwrap() {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('q') => {
                            match to_receiver.send("quit".to_string()) {
                                _ => {}
                            }
                            match to_transmiter.send("quit".to_string()) {
                                _ => {}
                            }
                            break 'ThreadLoop;
                        }
                        KeyCode::Char('p') => match to_receiver.send("pause".to_string()) {
                            _ => {}
                        },
                        KeyCode::Char('r') => match to_receiver.send("restore".to_string()) {
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }

            match to_main.recv_timeout(Duration::from_millis(10)) {
                Ok(message) => {
                    if message.contains("quiting") {
                        break 'ThreadLoop;
                    }
                }
                _ => {}
            }
        }

        if self.dir == Direction::Receive || self.dir == Direction::SendReceive {
            receive_handle.join().unwrap();
        }

        if self.dir == Direction::Send || self.dir == Direction::SendReceive {
            transmit_handle.join().unwrap();
        }
    }
}

fn main() {
    let args: Args = Args::parse();
    let mut application: App = App::new();

    if args.direct.send == false && args.direct.receive == true {
        application.dir = Direction::Receive;
    } else if args.direct.send == true && args.direct.receive == false {
        application.dir = Direction::Send;
    } else if args.direct.send == true && args.direct.receive == true {
        application.dir = Direction::SendReceive;
    } else {
        application.dir = Direction::None;
    }

    application.ip_addr.set_ip(
        IpAddr::from_str(&args.ip_address).expect("Incorrect IP address"));

    application.ip_addr.set_port(args.port);

    if args.timeout.is_some() {
        application.receive_timeout = args.timeout.unwrap();
    }

    match application.dir {
        Direction::None => return,
        _ => application.run(),
    }
}
