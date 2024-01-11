#![feature(offset_of)]
use mutant::try_cast;

fn main() {
  println!(
    "{:?}",
    try_cast::<[u8], hci::Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69])
  );
  println!(
    "{:?}",
    try_cast::<[u8], hci::Event<[u8]>>(&[21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69, 1, 2, 3])
  );
  println!(
    "{:?}",
    try_cast::<[u8], hci::Event<hci::ConnectionComplete>>(&[
      21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_cast::<[u8], hci::Event<hci::DisconnectionComplete>>(&[
      22, 9, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_cast::<[u8], hci::Event<hci::ConnectionComplete>>(&[
      22, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:?}",
    try_cast::<[u8], hci::Packet<hci::Event<[u8]>>>(&[
      2, 21, 9, 69, 69, 69, 69, 69, 69, 69, 69, 69
    ])
  );
  println!(
    "{:#?}",
    try_cast::<[u8], hci::Packet<hci::Event<hci::NumberOfCompletedPackets>>>(&[
      2, 23, 9, 2, 1, 0, 2, 0, 3, 0, 4, 0
    ])
  );
}

mod host {
  use super::*;
  use core::future::Future;
  use core::pin::pin;
  use mutant::{DataType, unaligned_ref};

  pub struct Host<'rx, L: hci::Socket>
    where
      Self: 'rx,
  {
    lower: L,
    rx: unsync::Slot<&'rx hci::Packet<[u8]>>,
  }

  impl<'rx, L: hci::Socket> hci::Socket for Host<'rx, L> {
    async fn recv(&self) -> &hci::Packet<[u8]> {
      self.rx.take().await
    }

    async fn send<T: ?Sized + DataType>(&self, tx: &hci::Packet<T>) {
      // TODO: flow-control
      self.lower.send(tx).await
    }
  }

  impl<'rx, L: hci::Socket> Host<'rx, L> {
    pub async fn get<T: ?Sized + DataType>(&self) -> &T {
      loop {
        match self.rx.get().await.try_cast::<hci::Packet<T>>() {
          Ok(value) => break unaligned_ref!(&value.data),
          _ => {}
        }
      }
    }

    pub async fn get_event<T: ?Sized + hci::EventParameters>(&self) -> &T {
      loop {
        match self.rx.get().await.try_cast::<hci::Packet<hci::Event<T>>>() {
          Ok(value) => {
            break {
              let data = unaligned_ref!(&value.data);
              unaligned_ref!(&data.parameters)
            }
          }
          _ => {}
        }
      }
    }
  }

  pub async fn host<F: Future, H: hci::Socket>(
    hci: H,
    f: impl FnOnce(&Host<'_, H>) -> F,
  ) -> F::Output {
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

  pub async fn accept<T: hci::Socket, F: Future>(
    host: &Host<'_, T>,
    f: impl FnOnce(u16) -> F,
  ) -> Option<F::Output> {
    let connection_handle = host.get_event::<hci::ConnectionComplete>().await.handle;
    let mut f = pin!(f(connection_handle));
    loop {
      // Poll `f` once.
      if let Some(value) = unsync::poll_once(f.as_mut()).await {
        break Some(value);
      }
      let disconnection = host.get_event::<hci::DisconnectionComplete>().await;
      if disconnection.handle == connection_handle {
        break None;
      }
    }
  }
}

mod hci {
  use core::fmt;
  use core::fmt::{Debug, Formatter};

  use mutant::{DataType, Undrop, Unalign, unaligned_ref};

  pub trait Socket {
    async fn recv(&self) -> &Packet<[u8]>;
    async fn send<T: ?Sized + DataType>(&self, tx: &Packet<T>);
  }

  #[derive(Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct Packet<T: ?Sized> {
    pub r#type: u8,
    pub data: Undrop<T>,
  }

  impl<T: ?Sized + Debug + DataType> Debug for Packet<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Packet")
        .field("type", &self.r#type)
        .field("data", &unaligned_ref!(&self.data))
        .finish()
    }
  }

  #[derive(Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct Acl<T: ?Sized> {
    pub handle_and_flags: Unalign<u16>,
    pub data_total_len: Unalign<u16>,
    pub parameters: Undrop<T>,
  }

  impl<T: ?Sized + Debug + DataType> Debug for Acl<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Acl")
        .field("handle_and_flags", &self.handle_and_flags)
        .field("data_total_len", &self.data_total_len)
        .field("completed_packets", &unaligned_ref!(&self.parameters))
        .finish()
    }
  }

  #[derive(Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct Event<T: ?Sized + EventParameters> {
    #[mutant(expect(T::EVENT_CODE))]
    pub event_code: u8,
    #[mutant(size)]
    pub parameters_len: u8,
    pub parameters: Undrop<T>,
  }

  pub trait EventParameters: DataType {
    const EVENT_CODE: Option<u8>;
  }

  impl EventParameters for [u8] {
    const EVENT_CODE: Option<u8> = None;
  }

  impl EventParameters for ConnectionComplete {
    const EVENT_CODE: Option<u8> = Some(21);
  }

  impl EventParameters for DisconnectionComplete {
    const EVENT_CODE: Option<u8> = Some(22);
  }

  impl EventParameters for NumberOfCompletedPackets {
    const EVENT_CODE: Option<u8> = Some(23);
  }

  #[derive(Debug, Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct ConnectionComplete {
    pub address: [u8; 6],
    pub handle: u16,
    pub link_type: u8,
  }

  #[derive(Debug, Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct DisconnectionComplete {
    pub address: [u8; 6],
    pub handle: u16,
  }

  #[derive(Debug, Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct CompletedPackets {
    pub handle: u16,
    pub number_of_completed_packets: u16,
  }

  #[derive(DataType)]
  #[repr(C, packed)]
  pub struct NumberOfCompletedPackets {
    pub num_handles: u8,
    pub completed_packets: [CompletedPackets],
  }

  #[derive(Copy, Clone, DataType)]
  #[repr(C, packed)]
  pub struct Command<T: ?Sized + DataType> {
    pub op_code: u16,
    pub parameters_len: u8,
    pub parameters: Undrop<T>,
  }

  impl<T: ?Sized + Debug + EventParameters> Debug for Event<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("Event")
        .field("event_code", &self.event_code)
        .field("parameters_len", &self.parameters_len)
        .field("parameters", &unaligned_ref!(&self.parameters))
        .finish()
    }
  }

  impl Debug for NumberOfCompletedPackets {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.debug_struct("NumberOfCompletedPackets")
        .field("num_handles", &self.num_handles)
        .field("completed_packets", &unaligned_ref!(&self.completed_packets))
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
