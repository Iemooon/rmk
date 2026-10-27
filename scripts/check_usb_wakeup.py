"""Exercise the actual patched USB writer and session-invalidation predicate."""
import argparse
from pathlib import Path
import subprocess
def function(text, name):
    start = text.index('fn ' + name + '(')
    begin = text.index('{', start)
    depth = 1
    end = begin + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]

ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('rmk',type=Path,nargs='?',default=ROOT/'rmk');args=parser.parse_args()
    src=args.rmk/'src/usb/mod.rs';text=src.read_text(encoding='utf-8')
    a=text.index('    pub(crate) async fn run_writer');b=text.index('    async fn write_composite',a)
    method=text[a:b]
    state=(args.rmk/'src/state.rs').read_text(encoding='utf-8')
    helper=function(state,'usb_report_session_ended')
    assert 'set_usb_state(UsbState::Enabled);' in text[text.index('    fn reset(&mut self) {'):text.index('    fn addressed(')]
    assert 'if usb_report_session_ended(prev, new)' in state
    out=ROOT/'target/usb-wakeup-tests'/args.rmk.name;out.mkdir(parents=True,exist_ok=True)
    code=r'''#![allow(dead_code,unused_variables)]
use std::{cell::{Cell,RefCell},collections::VecDeque,future::Future,task::{Context,Poll,Wake,Waker},sync::Arc};
#[derive(Clone,Copy,PartialEq)]enum UsbState{Configured,Suspended,Enabled,Disabled}
#[derive(Clone,Copy,PartialEq)]enum ConnectionType{Usb,Ble}
#[derive(Clone,Copy)]struct ConnectionStatus{usb:UsbState,active:Option<ConnectionType>}
impl ConnectionStatus{fn decide_active(&self)->Option<ConnectionType>{self.active}}
#[derive(Debug)]enum EndpointError{Disabled}
#[derive(Debug)]enum HidError{UsbEndpointError(EndpointError),Other}
thread_local!{
 static STATE:Cell<UsbState>=const{Cell::new(UsbState::Configured)};
 static EPOCH:Cell<u32>=const{Cell::new(0)};
 static NOW:Cell<u64>=const{Cell::new(0)};
 static WAKES:Cell<usize>=const{Cell::new(0)};
 static FAIL:Cell<usize>=const{Cell::new(0)};
 static BLOCK:Cell<bool>=const{Cell::new(false)};
 static QUEUE:RefCell<VecDeque<u8>>=const{RefCell::new(VecDeque::new())};
 static WRITTEN:RefCell<Vec<u8>>=const{RefCell::new(Vec::new())};
}
mod state{pub fn usb_report_epoch()->u32{super::EPOCH.with(std::cell::Cell::get)}}
fn current_usb_state()->UsbState{STATE.with(Cell::get)}
struct Channel;static USB_REPORT_CHANNEL:Channel=Channel;
impl Channel{async fn receive(&self)->u8{std::future::poll_fn(|cx|self.poll_receive(cx)).await}fn poll_receive(&self,_:&mut Context<'_>)->Poll<u8>{QUEUE.with(|q|q.borrow_mut().pop_front().map_or(Poll::Pending,Poll::Ready))}}
struct Signal;static USB_REMOTE_WAKEUP:Signal=Signal;
impl Signal{fn signal(&self,_:()){WAKES.with(|v|v.set(v.get()+1));}}
mod embassy_time{pub struct Timer;impl Timer{pub async fn after_millis(ms:u64){let deadline=super::NOW.with(std::cell::Cell::get)+ms;std::future::poll_fn(|_|if super::NOW.with(std::cell::Cell::get)>=deadline{std::task::Poll::Ready(())}else{std::task::Poll::Pending}).await}}}
enum Either<A,B>{First(A),Second(B)}
async fn select<A:Future,B:Future>(a:A,b:B)->Either<A::Output,B::Output>{
 let mut a=std::pin::pin!(a);let mut b=std::pin::pin!(b);
 std::future::poll_fn(|cx|{if let Poll::Ready(v)=a.as_mut().poll(cx){return Poll::Ready(Either::First(v));}if let Poll::Ready(v)=b.as_mut().poll(cx){return Poll::Ready(Either::Second(v));}Poll::Pending}).await
}
macro_rules! error{($($t:tt)*)=>{()};}
struct Writer;
impl Writer{
 async fn write_report(&mut self,r:&u8)->Result<usize,HidError>{
  if FAIL.with(Cell::get)>0{FAIL.with(|v|v.set(v.get()-1));return Err(HidError::UsbEndpointError(EndpointError::Disabled));}
  std::future::poll_fn(|_|if BLOCK.with(Cell::get){Poll::Pending}else{Poll::Ready(())}).await;
  WRITTEN.with(|v|v.borrow_mut().push(*r));Ok(1)
 }
'''+method+'}\n'+helper+r'''
struct NoWake;impl Wake for NoWake{fn wake(self:Arc<Self>){}}
fn exercise(f:impl FnOnce(&mut dyn FnMut())){
 STATE.with(|v|v.set(UsbState::Configured));EPOCH.with(|v|v.set(0));NOW.with(|v|v.set(0));WAKES.with(|v|v.set(0));FAIL.with(|v|v.set(0));BLOCK.with(|v|v.set(false));
 QUEUE.with(|v|*v.borrow_mut()=(0..6).collect());WRITTEN.with(|v|v.borrow_mut().clear());
 let waker=Waker::from(Arc::new(NoWake));let mut cx=Context::from_waker(&waker);
 let mut writer=Writer;let mut task=std::pin::pin!(writer.run_writer());
 f(&mut||assert!(task.as_mut().poll(&mut cx).is_pending()));
}
fn writes()->Vec<u8>{WRITTEN.with(|v|v.borrow().clone())}
#[test]fn normal_reports_are_in_order(){exercise(|poll|{poll();assert_eq!(writes(),vec![0,1,2,3,4,5]);assert_eq!(WAKES.with(Cell::get),0);});}
#[test]fn long_suspend_retains_first_press_and_all_releases(){exercise(|poll|{
 STATE.with(|v|v.set(UsbState::Suspended));poll();assert!(writes().is_empty());assert_eq!(QUEUE.with(|v|v.borrow().len()),5);
 for t in [5,500,2000]{NOW.with(|v|v.set(t));poll();assert!(writes().is_empty());}
 assert_eq!(WAKES.with(Cell::get),1);
 STATE.with(|v|v.set(UsbState::Configured));NOW.with(|v|v.set(2005));poll();assert_eq!(writes(),vec![0,1,2,3,4,5]);
});}
#[test]fn endpoint_disabled_race_retries_same_report(){exercise(|poll|{FAIL.with(|v|v.set(2));poll();assert!(writes().is_empty());NOW.with(|v|v.set(5));poll();NOW.with(|v|v.set(10));poll();assert_eq!(writes(),vec![0,1,2,3,4,5]);assert_eq!(WAKES.with(Cell::get),1);});}
#[test]fn unplug_replug_rejects_old_pending_and_accepts_new_first_key(){exercise(|poll|{
 STATE.with(|v|v.set(UsbState::Suspended));poll();STATE.with(|v|v.set(UsbState::Disabled));EPOCH.with(|v|v.set(1));QUEUE.with(|v|v.borrow_mut().clear());NOW.with(|v|v.set(5));poll();assert!(writes().is_empty());
 STATE.with(|v|v.set(UsbState::Configured));QUEUE.with(|v|v.borrow_mut().extend([8,9]));poll();assert_eq!(writes(),vec![8,9]);
});}
#[test]fn output_switch_cancels_blocked_old_write_and_keeps_release(){exercise(|poll|{
 BLOCK.with(|v|v.set(true));poll();EPOCH.with(|v|v.set(1));QUEUE.with(|v|{v.borrow_mut().clear();v.borrow_mut().push_back(99);});NOW.with(|v|v.set(5));poll();assert!(writes().is_empty());
 BLOCK.with(|v|v.set(false));poll();assert_eq!(writes(),vec![99]);
});}
#[test]fn simultaneous_endpoint_ready_and_old_session_end_never_replays_old_press(){exercise(|poll|{
 BLOCK.with(|v|v.set(true));poll();EPOCH.with(|v|v.set(1));QUEUE.with(|v|{v.borrow_mut().clear();v.borrow_mut().push_back(99);});
 BLOCK.with(|v|v.set(false));NOW.with(|v|v.set(1));poll();assert_eq!(writes(),vec![99]);
});}
#[test]fn session_boundary_distinguishes_resume_from_reenumeration(){
 let s=|usb,active|ConnectionStatus{usb,active};let u=Some(ConnectionType::Usb);let b=Some(ConnectionType::Ble);
 assert!(!usb_report_session_ended(s(UsbState::Configured,u),s(UsbState::Suspended,u)));
 assert!(!usb_report_session_ended(s(UsbState::Suspended,u),s(UsbState::Configured,u)));
 assert!(usb_report_session_ended(s(UsbState::Configured,u),s(UsbState::Enabled,None)));
 assert!(usb_report_session_ended(s(UsbState::Suspended,u),s(UsbState::Disabled,None)));
 assert!(usb_report_session_ended(s(UsbState::Configured,u),s(UsbState::Configured,b)));
 assert!(!usb_report_session_ended(s(UsbState::Enabled,None),s(UsbState::Configured,u)));
}
'''
    (out/'writer.rs').write_text(code,encoding='utf-8');exe=out/'writer-tests.exe'
    subprocess.run(['rustc','--edition=2024','--test',out/'writer.rs','-o',exe],check=True)
    print('Testing production writer:',src,flush=True);subprocess.run([exe],check=True)
if __name__=='__main__':main()
