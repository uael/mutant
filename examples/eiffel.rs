use mutant::num::*;
use mutant::*;

fn main() {
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
  );
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Event<[u8]>>(&[
      21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3
    ])
  );
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Event<hci::ConnectionComplete>>(&[
      21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Event<hci::DisconnectionComplete>>(&[
      22, 9, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Event<hci::ConnectionComplete>>(&[
      22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_transmute_ref::<[u8], hci::Packet<hci::Event<[u8]>>>(&[
      2, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:#?}",
    try_transmute_ref::<[u8], hci::Packet<hci::Event<hci::NumberOfCompletedPackets>>>(&[
      2, 23, 9, 2, 1, 0, 2, 0, 3, 0, 4, 0
    ])
  );
}

mod host {
  use core::future::Future;
  use core::pin::pin;

  use super::*;
  use crate::hci::*;

  pub struct Host<'rx, L: Socket>
  where
    Self: 'rx,
  {
    lower: L,
    rx: unsync::Slot<&'rx Packet<[u8]>>,
  }

  impl<'rx, L: Socket> Socket for Host<'rx, L> {
    async fn recv(&self) -> &Packet<[u8]> {
      self.rx.take().await
    }

    async fn send<T: ?Sized + PacketData>(&self, tx: &Packet<T>) {
      // TODO: flow-control
      self.lower.send(tx).await
    }
  }

  impl<'rx, L: Socket> Host<'rx, L> {
    pub async fn get<T: ?Sized + PacketData>(&self) -> &T {
      loop {
        if let Ok(value) = try_transmute_ref::<_, Packet<T>>(self.rx.get().await) {
          break unaligned_ref!(value.data);
        }
      }
    }

    pub async fn get_event<T: ?Sized + EventData>(&self) -> &T {
      loop {
        if let Ok(value) = try_transmute_ref::<_, Packet<Event<T>>>(self.rx.get().await) {
          break unaligned_ref!(value.data.parameters);
        }
      }
    }

    pub async fn command<T: CommandData>(&self, command: Command<T>) -> &T::ResultParameters {
      let packet = Packet { r#type: 0, data: Dropless::new(command) };
      self.send(&packet).await;
      // futures::future::select(
      //   self.get_event::<CommandComplete<T::ResultParameters>>()
      //     .then(|evt| async move { unaligned_ref!(evt.result_parameters).deref() }),
      //   self.get_event::<CommandStatus>()
      //     .then(|evt| async move { transmute_ref(&()) })
      // ).await
      todo!()
    }
  }

  pub async fn host<F: Future, H: Socket>(hci: H, f: impl FnOnce(&Host<'_, H>) -> F) -> F::Output {
    let host = Host { lower: hci, rx: Default::default() };
    let mut f = pin!(f(&host));
    loop {
      // Poll `f` once.
      if let Some(value) = unsync::poll_once(f.as_mut()).await {
        break value;
      }
      host.rx.set(host.lower.recv().await).await;
    }
  }

  pub async fn accept<T: Socket, F: Future>(
    host: &Host<'_, T>,
    f: impl FnOnce(u16le) -> F,
  ) -> Option<F::Output> {
    let connection_handle = host.get_event::<ConnectionComplete>().await.handle;
    let mut f = pin!(f(connection_handle));
    loop {
      // Poll `f` once.
      if let Some(value) = unsync::poll_once(f.as_mut()).await {
        break Some(value);
      }
      let disconnection = host.get_event::<DisconnectionComplete>().await;
      if disconnection.handle == connection_handle {
        break None;
      }
    }
  }
}

mod hci {
  use super::*;
  use core::fmt;
  use core::fmt::{Debug, Formatter};

  pub trait Socket {
    async fn recv(&self) -> &Packet<[u8]>;
    async fn send<T: ?Sized + PacketData>(&self, tx: &Packet<T>);
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct Packet<T: ?Sized + PacketData> {
    pub r#type: u8,
    pub data: Dropless<T>,
  }

  pub trait PacketData: Data {
    const TYPE: Option<u8>;
  }

  impl PacketData for [u8] {
    const TYPE: Option<u8> = None;
  }

  impl<T: ?Sized + EventData> PacketData for Event<T> {
    const TYPE: Option<u8> = Some(1);
  }

  impl<T: ?Sized + Data> PacketData for Acl<T> {
    const TYPE: Option<u8> = Some(2);
  }

  impl<T: ?Sized + CommandData> PacketData for Command<T> {
    const TYPE: Option<u8> = Some(3);
  }

  impl<T: ?Sized + Debug + PacketData> Debug for Packet<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Packet")
        .field("type", &self.r#type)
        .field("data", &unaligned_ref!(self.data))
        .finish()
    }
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct Acl<T: ?Sized> {
    pub handle_and_flags: u16le,
    pub data_total_len: u16le,
    pub parameters: Dropless<T>,
  }

  impl<T: ?Sized + Debug + Data> Debug for Acl<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Acl")
        .field("handle_and_flags", &self.handle_and_flags)
        .field("data_total_len", &self.data_total_len)
        .field("completed_packets", &unaligned_ref!(self.parameters))
        .finish()
    }
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct Event<T: ?Sized + EventData> {
    pub event_code: u8,
    pub parameters_len: u8,
    pub parameters: Dropless<T>,
  }

  pub trait EventData: Data {
    const EVENT_CODE: Option<u8>;
  }

  impl EventData for [u8] {
    const EVENT_CODE: Option<u8> = None;
  }

  impl EventData for ConnectionComplete {
    const EVENT_CODE: Option<u8> = Some(21);
  }

  impl EventData for DisconnectionComplete {
    const EVENT_CODE: Option<u8> = Some(22);
  }

  impl EventData for NumberOfCompletedPackets {
    const EVENT_CODE: Option<u8> = Some(23);
  }

  impl EventData for CommandStatus {
    const EVENT_CODE: Option<u8> = Some(24);
  }

  impl<T: ?Sized + Data> EventData for CommandComplete<T> {
    const EVENT_CODE: Option<u8> = Some(25);
  }

  #[derive(Debug, Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct CommandStatus {
    pub op_code: u16le,
    pub status: u8,
  }

  #[derive(Debug, Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct CommandComplete<T: ?Sized + Data> {
    pub op_code: u16le,
    pub status: u8,
    pub result_parameters: Dropless<T>,
  }

  #[derive(Debug, Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct ConnectionComplete {
    pub address: [u8; 6],
    pub handle: u16le,
    pub link_type: u8,
  }

  #[derive(Debug, Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct DisconnectionComplete {
    pub address: [u8; 6],
    pub handle: u16le,
  }

  #[derive(Debug, Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct CompletedPackets {
    pub handle: u16le,
    pub number_of_completed_packets: u16le,
  }

  #[derive(Data)]
  #[repr(C, packed)]
  pub struct NumberOfCompletedPackets {
    pub num_handles: u8,
    pub completed_packets: [CompletedPackets],
  }

  #[derive(Copy, Clone, Data)]
  #[repr(C, packed)]
  pub struct Command<T: ?Sized + CommandData> {
    pub op_code: u16le,
    pub parameters_len: u8,
    pub parameters: Dropless<T>,
  }

  pub trait CommandData: Data {
    type ResultParameters: ?Sized + Data;
    const OP_CODE: Option<u16le>;
  }

  impl<T: ?Sized + Debug + EventData> Debug for Event<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Event")
        .field("event_code", &self.event_code)
        .field("parameters_len", &self.parameters_len)
        .field("parameters", &unaligned_ref!(self.parameters))
        .finish()
    }
  }

  impl Debug for NumberOfCompletedPackets {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("NumberOfCompletedPackets")
        .field("num_handles", &self.num_handles)
        .field("completed_packets", &unaligned_ref!(self.completed_packets))
        .finish()
    }
  }
}

mod unsync {
  use core::cell::Cell;
  use core::future::{poll_fn, Future};
  use core::pin::pin;
  use core::task::Poll;

  pub async fn yield_once() {
    let mut pend = Some(());
    poll_fn(move |_| match pend.take() {
      None => Poll::Ready(()),
      Some(_) => Poll::Pending,
    })
    .await
  }

  pub async fn poll_once<F: Future>(f: F) -> Option<F::Output> {
    let mut f = pin!(f);
    poll_fn(move |cx| match f.as_mut().poll(cx) {
      Poll::Ready(value) => Poll::Ready(Some(value)),
      _ => Poll::Ready(None),
    })
    .await
  }

  #[repr(transparent)]
  pub struct Slot<T>(Cell<Option<T>>);

  impl<T> Slot<T> {
    pub const fn new(value: T) -> Self {
      Self(Cell::new(Some(value)))
    }

    pub const fn uninit() -> Self {
      Self(Cell::new(None))
    }

    pub fn is_set(&self) -> bool {
      match self.take_nowait() {
        Some(value) => {
          self.set_nowait(value);
          true
        }
        _ => false,
      }
    }

    pub fn set_nowait(&self, value: T) -> Option<T> {
      self.0.replace(Some(value))
    }

    pub fn take_nowait(&self) -> Option<T> {
      self.0.take()
    }

    pub async fn set(&self, value: T) {
      while self.is_set() {
        yield_once().await;
      }
      let none = self.set_nowait(value);
      assert!(none.is_none());
    }

    pub async fn take(&self) -> T {
      loop {
        yield_once().await;
        if let Some(value) = self.take_nowait() {
          break value;
        }
      }
    }
  }

  impl<T: Copy> Slot<T> {
    pub fn get_nowait(&self) -> Option<T> {
      self.0.get()
    }

    pub async fn get(&self) -> T {
      loop {
        yield_once().await;
        if let Some(value) = self.get_nowait() {
          break value;
        }
      }
    }
  }

  impl<T> Default for Slot<T> {
    fn default() -> Self {
      Self(Cell::default())
    }
  }
}
