// use core::mem::ManuallyDrop;
// use hci::*;
// use mutant::num::*;
// use mutant::*;
//
// fn main() {
//   let evt = Packet {
//     data: ManuallyDrop::new(Event {
//       parameters: ManuallyDrop::new(NumberOfCompletedPackets {
//         completed_packets: ManuallyDrop::new([
//           CompletedPackets { handle: 1.into(), number_of_completed_packets: 2.into() },
//           CompletedPackets { handle: 3.into(), number_of_completed_packets: 4.into() },
//         ]),
//         ..default()
//       }),
//       ..default()
//     }),
//     ..default()
//   };
//   assert_eq!(Ok(&evt), try_transmute_ref(transmute_ref::<_, [u8]>(&evt)));
//
//   let evt = Packet {
//     data: ManuallyDrop::new(Event {
//       parameters: ManuallyDrop::new(DisconnectionComplete {
//         address: [1, 2, 3, 4, 5, 6],
//         handle: 0x1234.into(),
//       }),
//       ..default()
//     }),
//     ..default()
//   };
//   assert_eq!(Ok(&evt), try_transmute_ref(transmute_ref::<_, [u8]>(&evt)));
//
//   let evt = Packet {
//     data: ManuallyDrop::new(Event {
//       parameters: ManuallyDrop::new(ConnectionComplete {
//         address: [1, 2, 3, 4, 5, 6],
//         handle: 0x1234.into(),
//         link_type: 0x56,
//       }),
//       ..default()
//     }),
//     ..default()
//   };
//   assert_eq!(Ok(&evt), try_transmute_ref(transmute_ref::<_, [u8]>(&evt)));
// }
//
// mod host {
//   use core::future::Future;
//   use core::pin::pin;
//   use futures::FutureExt;
//
//   use super::*;
//
//   pub struct Host<'rx, L: Socket>
//   where
//     Self: 'rx,
//   {
//     lower: L,
//     rx: unsync::Slot<&'rx Packet<[u8]>>,
//   }
//
//   impl<'rx, L: Socket> Socket for Host<'rx, L> {
//     async fn socket_recv(&self) -> &Packet<[u8]> {
//       self.rx.take().await
//     }
//
//     async fn socket_send<T: ?Sized + PacketData>(&self, tx: &Packet<T>) {
//       // TODO: flow-control
//       self.lower.socket_send(tx).await
//     }
//   }
//
//   impl<'rx, L: Socket> Host<'rx, L> {
//     pub async fn recv<T: ?Sized + PacketData>(&self) -> &T {
//       loop {
//         if let Ok(packet) = try_transmute_ref::<_, Packet<T>>(self.rx.get().await) {
//           break unaligned_ref!(packet.data);
//         }
//       }
//     }
//
//     pub async fn recv_event<T: ?Sized + EventData>(&self) -> &T {
//       loop {
//         if let Ok(packet) = try_transmute_ref::<_, Packet<Event<T>>>(self.rx.get().await) {
//           break unaligned_ref!(packet.data.parameters);
//         }
//       }
//     }
//
//     pub async fn send_command<T: CommandData + Copy>(
//       &self, command: Command<T>,
//     ) -> &T::ResultParameters {
//       self.socket_send(&Packet { data: ManuallyDrop::new(command), ..default() }).await;
//       futures::select_biased! {
//         e = self.recv_event::<CommandComplete<T>>().fuse() => unaligned_ref!(e.result_parameters),
//         _ = self.recv_event::<CommandStatus>().fuse() => transmute_ref(&())
//       }
//     }
//   }
//
//   pub async fn host<F: Future, H: Socket>(hci: H, f: impl FnOnce(&Host<'_, H>) -> F) -> F::Output {
//     let host = Host { lower: hci, rx: Default::default() };
//     let mut f = pin!(f(&host));
//     loop {
//       // Poll `f` once.
//       if let Some(value) = unsync::poll_once(f.as_mut()).await {
//         break value;
//       }
//       host.rx.set(host.lower.socket_recv().await).await;
//     }
//   }
//
//   pub async fn accept<T: Socket, F: Future>(
//     host: &Host<'_, T>, f: impl FnOnce(u16le) -> F,
//   ) -> Option<F::Output> {
//     let connection_handle = host.recv_event::<ConnectionComplete>().await.handle;
//     let mut disconnect_complete = pin!(host.recv_event::<DisconnectionComplete>());
//     let mut f = pin!(f(connection_handle));
//     loop {
//       if let Some(disconnect_complete) = unsync::poll_once(disconnect_complete.as_mut()).await {
//         if disconnect_complete.handle == connection_handle {
//           break None;
//         }
//       }
//       // Poll `f` once.
//       if let Some(value) = unsync::poll_once(f.as_mut()).await {
//         break Some(value);
//       }
//     }
//   }
// }
//
// mod hci {
//   use super::*;
//   use core::mem::ManuallyDrop;
//
//   pub trait Socket {
//     async fn socket_recv(&self) -> &Packet<[u8]>;
//     async fn socket_send<T: ?Sized + PacketData>(&self, tx: &Packet<T>);
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct Packet<T: ?Sized + PacketData> {
//     pub r#type: T::PacketType,
//     pub data: ManuallyDrop<T>,
//   }
//
//   pub trait PacketData: Data {
//     type PacketType: Integer<Repr = u8>;
//   }
//
//   impl PacketData for [u8] {
//     type PacketType = u8;
//   }
//
//   impl<T: ?Sized + CommandData> PacketData for Command<T> {
//     type PacketType = u8c<3>;
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct Acl<T: ?Sized> {
//     pub handle_and_flags: u16le,
//     pub data_total_len: u16le,
//     pub parameters: ManuallyDrop<T>,
//   }
//
//   impl<T: ?Sized + Data> PacketData for Acl<T> {
//     type PacketType = u8c<2>;
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct Event<T: ?Sized + EventData> {
//     pub event_code: T::EventCode,
//     pub parameters_len: u8,
//     pub parameters: ManuallyDrop<T>,
//   }
//
//   impl<T: ?Sized + EventData> PacketData for Event<T> {
//     type PacketType = u8c<1>;
//   }
//
//   pub trait EventData: Data {
//     type EventCode: Integer<Repr = u8>;
//   }
//
//   impl EventData for [u8] {
//     type EventCode = u8;
//   }
//
//   impl EventData for ConnectionComplete {
//     type EventCode = u8c<21>;
//   }
//
//   impl EventData for DisconnectionComplete {
//     type EventCode = u8c<22>;
//   }
//
//   impl<T: ?Sized + Data<Item = CompletedPackets>> EventData for NumberOfCompletedPackets<T> {
//     type EventCode = u8c<23>;
//   }
//
//   impl EventData for CommandStatus {
//     type EventCode = u8c<24>;
//   }
//
//   impl<T: ?Sized + CommandData> EventData for CommandComplete<T> {
//     type EventCode = u8c<25>;
//   }
//
//   #[derive(Debug, Copy, Clone, PartialEq, Data)]
//   #[repr(C, packed)]
//   pub struct CommandStatus {
//     pub op_code: u16le,
//     pub status: u8,
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct CommandComplete<T: ?Sized + CommandData> {
//     pub op_code: T::OpCode,
//     pub status: u8,
//     pub result_parameters: ManuallyDrop<T::ResultParameters>,
//   }
//
//   #[derive(Debug, Copy, Clone, PartialEq, Data)]
//   #[repr(C, packed)]
//   pub struct ConnectionComplete {
//     pub address: [u8; 6],
//     pub handle: u16le,
//     pub link_type: u8,
//   }
//
//   #[derive(Debug, Copy, Clone, PartialEq, Data)]
//   #[repr(C, packed)]
//   pub struct DisconnectionComplete {
//     pub address: [u8; 6],
//     pub handle: u16le,
//   }
//
//   #[derive(Debug, Copy, Clone, PartialEq, Data)]
//   #[repr(C, packed)]
//   pub struct CompletedPackets {
//     pub handle: u16le,
//     pub number_of_completed_packets: u16le,
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct NumberOfCompletedPackets<
//     T: ?Sized + Data<Item = CompletedPackets> = [CompletedPackets],
//   > {
//     pub num_handles: u8,
//     pub completed_packets: ManuallyDrop<T>,
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data, DebugPacked)]
//   #[repr(C, packed)]
//   pub struct Command<T: ?Sized + CommandData> {
//     pub op_code: T::OpCode,
//     pub parameters_len: u8,
//     pub parameters: ManuallyDrop<T>,
//   }
//
//   pub trait CommandData: Data {
//     type OpCode: Integer<Repr = u16le>;
//     type ResultParameters: ?Sized + Data;
//   }
//
//   #[derive(Copy, Clone, PartialEq, Data)]
//   pub struct Reset;
//
//   impl CommandData for Reset {
//     type OpCode = u16lec<0x2345>;
//     type ResultParameters = ();
//   }
// }
//
// mod unsync {
//   use core::cell::Cell;
//   use core::future::{poll_fn, Future};
//   use core::pin::pin;
//   use core::task::Poll;
//
//   pub async fn yield_once() {
//     let mut pend = Some(());
//     poll_fn(move |_| match pend.take() {
//       None => Poll::Ready(()),
//       Some(_) => Poll::Pending,
//     })
//     .await
//   }
//
//   pub async fn poll_once<F: Future>(f: F) -> Option<F::Output> {
//     let mut f = pin!(f);
//     poll_fn(move |cx| match f.as_mut().poll(cx) {
//       Poll::Ready(value) => Poll::Ready(Some(value)),
//       _ => Poll::Ready(None),
//     })
//     .await
//   }
//
//   #[repr(transparent)]
//   pub struct Slot<T>(Cell<Option<T>>);
//
//   impl<T> Slot<T> {
//     pub const fn new(value: T) -> Self {
//       Self(Cell::new(Some(value)))
//     }
//
//     pub const fn uninit() -> Self {
//       Self(Cell::new(None))
//     }
//
//     pub fn is_set(&self) -> bool {
//       match self.take_nowait() {
//         Some(value) => {
//           self.set_nowait(value);
//           true
//         }
//         _ => false,
//       }
//     }
//
//     pub fn set_nowait(&self, value: T) -> Option<T> {
//       self.0.replace(Some(value))
//     }
//
//     pub fn take_nowait(&self) -> Option<T> {
//       self.0.take()
//     }
//
//     pub async fn set(&self, value: T) {
//       while self.is_set() {
//         yield_once().await;
//       }
//       let none = self.set_nowait(value);
//       assert!(none.is_none());
//     }
//
//     pub async fn take(&self) -> T {
//       loop {
//         yield_once().await;
//         if let Some(value) = self.take_nowait() {
//           break value;
//         }
//       }
//     }
//   }
//
//   impl<T: Copy> Slot<T> {
//     pub fn get_nowait(&self) -> Option<T> {
//       self.0.get()
//     }
//
//     pub async fn get(&self) -> T {
//       loop {
//         yield_once().await;
//         if let Some(value) = self.get_nowait() {
//           break value;
//         }
//       }
//     }
//   }
//
//   impl<T> Default for Slot<T> {
//     fn default() -> Self {
//       Self(Cell::default())
//     }
//   }
// }

fn main() {

}
