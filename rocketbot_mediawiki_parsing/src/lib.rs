mod droppable_child;


use std::error;
use std::fmt;
use std::num::TryFromIntError;
use std::process::Command;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpStream};
use std::string::FromUtf8Error;

use crate::droppable_child::DroppableChild;


const PORT: u16 = 10101;
const MAGIC: &[u8] = b"WiKiCrUnCh";
const TEMPLATE_MAGIC: &[u8] = b"WiKiTeMpL8";
const STOP_MAGIC: &[u8] = b"EnOuGhWiKi";


#[derive(Debug)]
pub enum ParserError {
    LengthDoesNotFit(TryFromIntError),
    DataTransfer(io::Error),
    Utf8Decoding(FromUtf8Error),
}
impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthDoesNotFit(e) => write!(f, "length does not fit: {}", e),
            Self::DataTransfer(e) => write!(f, "data transfer failed: {}", e),
            Self::Utf8Decoding(e) => write!(f, "UTF-8 decoding failed: {}", e),
        }
    }
}
impl error::Error for ParserError {
}
impl From<io::Error> for ParserError {
    fn from(e: io::Error) -> Self {
        Self::DataTransfer(e)
    }
}


pub struct WikiParser {
    _parser: Option<DroppableChild>,
    socket: Option<TcpStream>,
}
impl WikiParser {
    pub fn new(php_command: &str, wiki_parse_server_dir: &str) -> Result<WikiParser, io::Error> {
        // launch the parser
        let parser_child = Command::new(php_command)
            .arg("wikiparseserver.php")
            .arg(PORT.to_string())
            .current_dir(wiki_parse_server_dir)
            .spawn()?;
        let parser = Some(parser_child.into());
        let socket = None;
        Ok(Self {
            _parser: parser,
            socket,
        })
    }

    pub fn new_existing() -> WikiParser {
        let parser = None;
        let socket = None;
        Self {
            _parser: parser,
            socket,
        }
    }

    fn ensure_open_socket(&mut self) -> Result<(), io::Error> {
        if self.socket.is_none() {
            let socket = TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT))?;
            self.socket = Some(socket);
        }
        Ok(())
    }

    pub fn supply_template(&mut self, title: &str, wikitext: &str) -> Result<(), ParserError> {
        self.ensure_open_socket()?;

        let title_length_u32: u32 = title.len().try_into()
            .map_err(|e| ParserError::LengthDoesNotFit(e))?;
        let wikitext_length_u32: u32 = wikitext.len().try_into()
            .map_err(|e| ParserError::LengthDoesNotFit(e))?;

        let socket = self.socket.as_mut().unwrap();

        socket.write_all(TEMPLATE_MAGIC)?;

        socket.write_all(&title_length_u32.to_be_bytes())?;
        socket.write_all(title.as_bytes())?;

        socket.write_all(&wikitext_length_u32.to_be_bytes())?;
        socket.write_all(wikitext.as_bytes())?;

        Ok(())
    }

    pub fn parse_article(&mut self, title: &str, wikitext: &str) -> Result<String, ParserError> {
        self.ensure_open_socket()?;

        let title_length_u32: u32 = title.len().try_into()
            .map_err(|e| ParserError::LengthDoesNotFit(e))?;
        let wikitext_length_u32: u32 = wikitext.len().try_into()
            .map_err(|e| ParserError::LengthDoesNotFit(e))?;

        let socket = self.socket.as_mut().unwrap();

        socket.write_all(MAGIC)?;

        socket.write_all(&title_length_u32.to_be_bytes())?;
        socket.write_all(title.as_bytes())?;

        socket.write_all(&wikitext_length_u32.to_be_bytes())?;
        socket.write_all(wikitext.as_bytes())?;

        let mut parsed_length_buf = [0u8; 4];
        socket.read_exact(&mut parsed_length_buf)?;
        let parsed_length: usize = u32::from_be_bytes(parsed_length_buf).try_into()
            .map_err(|e| ParserError::LengthDoesNotFit(e))?;

        let mut parsed_buf = vec![0u8; parsed_length];
        socket.read_exact(&mut parsed_buf)?;

        let string = String::from_utf8(parsed_buf)
            .map_err(|e| ParserError::Utf8Decoding(e))?;

        Ok(string)
    }

    pub fn parsing_done(&mut self) -> Result<(), io::Error> {
        self.ensure_open_socket()?;

        {
            let socket = self.socket.as_mut().unwrap();
            socket.write_all(STOP_MAGIC)?;
            socket.shutdown(Shutdown::Write)?;
        }
        self.socket = None;

        Ok(())
    }
}
