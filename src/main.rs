use clap::Parser;
use core::fmt;
#[allow(unused_imports)]
use std::net::{SocketAddr, UdpSocket, IpAddr, Ipv4Addr};
use std::str::FromStr;
#[allow(unused_imports)]
use std::thread::{self, JoinHandle, sleep};
#[allow(unused_imports)]
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use crossterm::event::{KeyCode, Event};
use crossterm::event;

pub const TICK_FOR_OPERATION_TIMEOUT: u64 = 10;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, author = "MK")]
struct Args {
    /// specifies if send thread needs to be started
    #[arg(short, long, group="direction")]
    send: bool,

    /// specifies if receive thread needs to be started
    #[arg(short, long, group="direction")]
    receive: bool,

    /// specifies port for UDP communication
    #[arg(short, long, requires = "direction")]
    port: Option<u16>,

    /// specifies IP address for UDP communication
    #[arg(short, long, requires = "direction")]
    ip_address: Option<String>,

    /// specifies timeout for receive thread
    #[arg(short, long, requires = "direction")]
    timeout: Option<u32>,
}

#[derive(Debug)]
#[derive(PartialEq)]
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
        write!(f, "timeout: {} \ndirection: {} \n", self.receive_timeout, {
            match self.dir {
                Direction::None => "no transmit".to_string(),
                Direction::Receive => "only receiving".to_string(),
                Direction::Send => "only transmiting".to_string(),
                Direction::SendReceive => "sending and receiving".to_string(),
            }
        })
    }
}

impl App{
    fn new() -> App {
        App {
            dir: Direction::None,
            ip_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 62345),
            receive_timeout: 10,
        }
    }

    fn transmit_thread(msg_to_me: &std::sync::mpsc::Receiver<String>, to_main: &std::sync::mpsc::SyncSender<String>, timeout: u32, ip_address: SocketAddr) {
        let start = Instant::now();
        let time_out = Duration::from_secs(timeout as u64);
        let mut count = 0;

        println!("Transmiting ....");

        let socket:UdpSocket;
        let mut tx_sock_addr = ip_address.clone();
        tx_sock_addr.set_port(tx_sock_addr.port() + 1);

        match UdpSocket::bind(tx_sock_addr) {
            Ok(sock) => socket = sock,
            Err (_) => {
                println!("Cannot bind to the socket");
                println!("Time spent in thread: {:?}", start.elapsed());
                to_main.send("quiting".to_string()).unwrap();
                return;        
            }
        };

        // socket.set_broadcast(true).expect("Failed to set broadcast");
        socket.connect(ip_address).unwrap();

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
            match socket.send(&mut message) {
                Ok(_) => {}
                Err(e) => println!("packtet {} dropped with error: {:?}", count - 1, e),
            }
            sleep(Duration::from_secs(1));
        }

        println!("Time spent in thread: {:?}", start.elapsed());
        to_main.send("quiting".to_string()).unwrap();
    }

    fn receive_thread(msg_to_me: &std::sync::mpsc::Receiver<String>, to_main: &std::sync::mpsc::SyncSender<String>, timeout: u32, ip_address: SocketAddr) {
        let start = Instant::now();
        let time_out = Duration::from_secs(timeout as u64);

        println!("Receiving ....");

        let socket:UdpSocket;
        let mut tx_socket = ip_address;
        tx_socket.set_port(tx_socket.port() + 1);

        match UdpSocket::bind(ip_address) {
            Ok(sock) => socket = sock,
            Err (_) => {
                println!("Cannot bind to the socket");
                println!("Time spent in thread: {:?}", start.elapsed());
                to_main.send("quiting".to_string()).unwrap();
                return;        
            }
        };
        socket.set_read_timeout(Some(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT))).unwrap();
        socket.connect(tx_socket).unwrap();

        let mut rx_buf = vec![0u8; 4096];
        rx_buf.clear();

        'klop: loop {
            match msg_to_me.recv_timeout(Duration::from_millis(TICK_FOR_OPERATION_TIMEOUT)) {
                Ok(message) => {
                    if message.contains("quit") {
                        break;
                    }        
                }
                _ => {}
            }
        
            match socket.recv(& mut rx_buf) {
                Ok(size) => {
                    if size != 0 {
                        println!("Received: {:?}", rx_buf);
                        rx_buf.clear();
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
            receive_handle = std::thread::spawn(move || { Self::receive_thread(&for_receiver, &form_threads, self.receive_timeout, self.ip_addr) });
        } else {
            receive_handle = std::thread::spawn(move || { });
        }

        if self.dir == Direction::Send || self.dir == Direction::SendReceive {
            println!("Starting transmiting thread ....");
            transmit_handle = std::thread::spawn(move || { Self::transmit_thread(&for_transmiter, &form_threads2, self.receive_timeout, self.ip_addr) });
        } else {
            transmit_handle = std::thread::spawn(move || { });
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

    if args.send == false && args.receive == true {
        application.dir = Direction::Receive;
    } else if args.send == true && args.receive == false {
        application.dir = Direction::Send;
    } else {
        application.dir = Direction::SendReceive;
    }

    if args.ip_address.is_some() {
        application.ip_addr.set_ip(IpAddr::from_str(&args.ip_address.expect("Incorrect IP address")).expect("Incorrect IP address"));
    }

    if args.port.is_some() {
        application.ip_addr.set_port(args.port.unwrap());
    }

    if args.timeout.is_some() {
        application.receive_timeout = args.timeout.unwrap();
    }

    match application.dir {
        Direction::None => return, 
        _ => application.run(),
    }

}
