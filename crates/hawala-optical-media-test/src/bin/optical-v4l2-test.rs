//! TEST ONLY: Linux V4L2 one-shot capture -> HWCAM001 YUYV422 packet.
//! No financial authority, signer, bank account, network or ATM executor.
//! The operator must explicitly select a trusted device and capture tool.
use hawala_optical_media_test::{encode_camera_packet,CaptureFrame,CaptureLayout,MAX_SIDE};
use std::fs::{self,File};
use std::io::{Read,Write};
use std::os::unix::{fs::FileTypeExt,process::CommandExt};
use std::path::Path;
use std::process::{Child,Command,Stdio};
use std::time::{Duration,Instant};
const DEFAULT_LIMIT:Duration=Duration::from_secs(5);
const HARD_LIMIT:Duration=Duration::from_secs(10);

fn dimensions(w:usize,h:usize)->Result<usize,String>{
    if w==0 || h==0 || w>MAX_SIDE || h>MAX_SIDE || !w.is_multiple_of(2){
        return Err("YUYV422 requires bounded even width".into());
    }
    w.checked_mul(h).and_then(|n|n.checked_mul(2))
        .ok_or_else(||"capture size overflow".into())
}
fn validate_device(path:&Path)->Result<(),String>{
    let name=path.to_str().ok_or("non UTF-8 camera path")?;
    let suffix=name.strip_prefix("/dev/video").ok_or("expect /dev/videoN")?;
    if suffix.is_empty() || !suffix.bytes().all(|b|b.is_ascii_digit()){
        return Err("invalid direct video device".into());
    }
    let kind=fs::symlink_metadata(path).map_err(|_|"camera not installed")?.file_type();
    if !kind.is_char_device() || kind.is_symlink(){
        return Err("not a direct video character device".into());
    }
    Ok(())
}
fn validate_tool(path:&Path)->Result<(),String>{
    if !path.is_absolute(){return Err("absolute tool path required".into());}
    let kind=fs::symlink_metadata(path).map_err(|_|"capture helper missing")?.file_type();
    if !kind.is_file() || kind.is_symlink(){
        return Err("capture helper must be a non-symlink regular file".into());
    }
    Ok(())
}
fn terminate(child:&mut Child){
    // The child starts a fresh process group. Never let a hung reader
    // outlive its bounded capture request.
    if let Ok(pid)=i32::try_from(child.id()){
        // SAFETY: libc kill receives the PID of the process group created
        // by process_group(0) in this helper's spawn path.
        unsafe {libc::kill(-pid,libc::SIGKILL);}
    }
    let _=child.kill();
    let _=child.wait();
}
/// Test can inject a dummy executable. The public CLI additionally validates
/// a physical /dev/videoN char device and operator-selected helper binary.
fn acquire(ctl:&Path,device:&str,w:usize,h:usize,limit:Duration)->Result<Vec<u8>,String>{
    let len=dimensions(w,h)?;
    if limit.is_zero() || limit>HARD_LIMIT{return Err("invalid deadline".into());}
    let dir=tempfile::tempdir().map_err(|_|"private spool failed")?;
    let frame=dir.path().join("one-frame.yuyv");
    let mut command=Command::new(ctl);
    command.arg("--device").arg(device)
        .arg(format!("--set-fmt-video=width={w},height={h},pixelformat=YUYV"))
        .arg("--stream-mmap=3")
        .arg("--stream-count=1")
        .arg(format!("--stream-to={}",frame.display()))
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .env_clear().env("LC_ALL","C").env("PATH","/usr/bin:/bin")
        .process_group(0);
    // Independent of any input-file length check after capture: a faulty
    // capture tool cannot create an unbounded spool or monopolize memory,
    // CPU, open files or core dumps within our TEST process contract.
    // This is resource containment, not syscall/device sandboxing.
    // SAFETY: pre_exec closure uses only async-signal-safe setrlimit calls.
    unsafe {
        command.pre_exec(move || {
            for (resource,limit) in [
                (libc::RLIMIT_FSIZE,len as u64),
                (libc::RLIMIT_AS,256*1024*1024u64),
                (libc::RLIMIT_CPU,3u64),
                (libc::RLIMIT_NOFILE,64u64),
                (libc::RLIMIT_CORE,0u64),
            ] {
                let budget=libc::rlimit{rlim_cur:limit,rlim_max:limit};
                if libc::setrlimit(resource,&budget)!=0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child=command.spawn().map_err(|_|"failed to launch capture helper")?;
    let started=Instant::now();
    let result=loop{
        match child.try_wait(){
            Ok(Some(exit))=>break exit,
            Ok(None)=>{},
            Err(_)=>{terminate(&mut child);return Err("helper wait failure".into());}
        }
        if started.elapsed()>=limit{
            terminate(&mut child);
            return Err("capture deadline exceeded".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if !result.success() || started.elapsed()>=limit {
        return Err("capture failed or was completed late".into());
    }
    let meta=fs::symlink_metadata(&frame).map_err(|_|"missing capture buffer")?;
    if !meta.file_type().is_file() || meta.len()!=len as u64 {
        return Err("unsupported padded/truncated capture".into());
    }
    // This temporary file is not encrypted or securely erased; only TEST.
    let mut bytes=Vec::with_capacity(len);
    File::open(&frame).map_err(|_|"unreadable frame")?
        .take((len+1) as u64)
        .read_to_end(&mut bytes).map_err(|_|"failed bounded frame read")?;
    if bytes.len()!=len{return Err("invalid frame length".into());}
    encode_camera_packet(&CaptureFrame{
        width:w,height:h,row_stride:w*2,
        layout:CaptureLayout::Yuyv422,bytes:&bytes,
    })
}
fn run()->Result<(),String>{
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=5 {
        return Err("usage: optical-v4l2-test /dev/videoN EVEN_WIDTH HEIGHT /abs/path/v4l2-ctl".into());
    }
    let w=args[2].parse::<usize>().map_err(|_|"invalid width")?;
    let h=args[3].parse::<usize>().map_err(|_|"invalid height")?;
    dimensions(w,h)?;
    validate_device(Path::new(&args[1]))?;
    validate_tool(Path::new(&args[4]))?;
    let bytes=acquire(Path::new(&args[4]),&args[1],w,h,DEFAULT_LIMIT)?;
    std::io::stdout().lock().write_all(&bytes)
        .map_err(|_|"camera packet stdout write failed".into())
}
fn main(){
    if let Err(message)=run(){
        eprintln!("TEST capture rejected: {message}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use hawala_optical_media_test::{parse_camera_packet,Raster};
    use std::os::unix::fs::PermissionsExt;
    fn fake(dir:&Path,body:&str)->std::path::PathBuf{
        let path=dir.join("fake-v4l2-ctl");
        fs::write(&path,format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut p=fs::metadata(&path).unwrap().permissions();
        p.set_mode(0o700);fs::set_permissions(&path,p).unwrap();
        path
    }
    const OUT:&str=r#"for arg; do case "$arg" in --stream-to=*) out=$(echo "$arg" | sed 's/^--stream-to=//');; esac; done"#;
    #[test]
    fn exact_mock_camera_frame_is_untrusted_ipc_packet(){
        let dir=tempfile::tempdir().unwrap();
        let path=fake(dir.path(),&format!("{OUT}\nhead -c 16 /dev/zero > \"$out\""));
        let bytes=acquire(&path,"/dev/video0",4,2,Duration::from_secs(2)).unwrap();
        let p=parse_camera_packet(&bytes).unwrap();
        assert_eq!((p.width,p.height,p.row_stride),(4,2,8));
        assert_eq!(p.layout,CaptureLayout::Yuyv422);
        assert_eq!(Raster::from_capture(&p).unwrap().pixels(),&[0u8;8]);
    }
    #[test]
    fn faulty_truncated_and_oversized_mock_camera_frames_fail(){
        let dir=tempfile::tempdir().unwrap();
        for count in [15,17] {
            let path=fake(dir.path(),&format!("{OUT}\nhead -c {count} /dev/zero > \"$out\""));
            assert!(acquire(&path,"/dev/video0",4,2,Duration::from_secs(2)).is_err());
        }
        let bad=fake(dir.path(),"exit 7");
        assert!(acquire(&bad,"/dev/video0",4,2,Duration::from_secs(2)).is_err());
    }
    #[test]
    fn slow_or_stuck_camera_has_a_hard_deadline(){
        let dir=tempfile::tempdir().unwrap();
        let path=fake(dir.path(),"sleep 4");
        let start=Instant::now();
        assert!(acquire(&path,"/dev/video0",4,2,Duration::from_millis(70)).is_err());
        assert!(start.elapsed()<Duration::from_secs(3));
    }
    #[test]
    fn arbitrary_devices_and_invalid_dimensions_are_refused(){
        for (w,h) in [(0,480),(641,480),(1026,480),(640,0),(640,1025)]{
            assert!(dimensions(w,h).is_err());
        }
        assert_eq!(dimensions(640,480).unwrap(),640*480*2);
        for p in ["/dev/null","/etc/passwd","video0","/dev/videoA","/dev/video"]{
            assert!(validate_device(Path::new(p)).is_err());
        }
        assert!(validate_tool(Path::new("v4l2-ctl")).is_err());
        assert!(acquire(Path::new("/missing"),"/dev/video0",4,2,Duration::ZERO).is_err());
    }
}
