//! The single owner of application-installed WhisperX environments and supervised children.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, sync::{Arc, OnceLock}, time::Duration};
use tauri::Manager;
use tokio::{io::{AsyncBufReadExt, BufReader}, process::{Child, Command}, sync::{Mutex, Semaphore, watch}};
use ts_rs::TS;
const SERVICE_VERSION: &str = "0.2.0";
const PROTOCOL: &str = "dsivio-video.asr/1";
/// Downloads take as long as the network needs; a step only fails when nothing moves for a while.
struct Watchdog {poll:Duration,stall:Duration,limit:Duration}
const INSTALL_WATCHDOG:Watchdog=Watchdog {poll:Duration::from_secs(5),stall:Duration::from_secs(600),limit:Duration::from_secs(4*3600)};
pub use crate::settings::LocalAsrConfig;
impl LocalAsrConfig {
    fn normalized(mut self)->Result<Self,String> {
        if self.model != "small" {return Err("ASR_MODEL_UNSUPPORTED: small required".into());}
        if self.languages.is_empty() {return Err("ASR_LANGUAGE_REQUIRED".into());}
        for language in &self.languages {super::transcribe_providers::validate_language(language)?;}
        self.languages.sort();self.languages.dedup();Ok(self)
    }
    fn same_install(&self,other:&Self)->bool {self.model==other.model && self.languages==other.languages}
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all="camelCase")]
pub struct RuntimeStatus {pub state:String,pub pid:Option<u32>,pub active_task_id:Option<String>}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all="camelCase")]
pub struct LocalAsrStopResult {pub outcome:String,pub runtime:RuntimeStatus}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all="camelCase")]
pub struct LocalAsrProgress {pub stage:String,pub message:String,#[ts(type = "number | null")] pub downloaded_bytes:Option<u64>}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all="camelCase")]
pub struct LocalAsrStatus {
    pub operation_id:Option<String>,pub state:String,pub installation_id:Option<String>,pub service_version:String,
    pub model:String,pub languages:Vec<String>,pub progress:Option<LocalAsrProgress>,pub error:Option<String>,pub runtime:RuntimeStatus,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Installed {installation_id:String,config:LocalAsrConfig,snapshot_hash:String}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Manifest {plugin_revision:String,protocol:String,service_version:String,whisperx_version:String,files:BTreeMap<String,String>}
struct InstallState {status:LocalAsrStatus,active:Option<Installed>,cancel:Option<watch::Sender<bool>>}
struct OwnedService {child:Child,port:u16,nonce:String,generation:String,active_task_id:Option<String>,last_used:std::time::Instant,run_dir:PathBuf,installation_dir:PathBuf}
struct Supervisor {root:PathBuf,snapshot:PathBuf,python:PathBuf,evidence:PathBuf,state:Mutex<InstallState>,service:Mutex<Option<OwnedService>>,startup:Mutex<Option<(String,String,watch::Sender<bool>)>>,queue:Arc<Semaphore>,shutting_down:std::sync::atomic::AtomicBool,client:reqwest::Client}
static OWNER:OnceLock<Arc<Supervisor>>=OnceLock::new();
fn owner()->Result<Arc<Supervisor>,String> {OWNER.get().cloned().ok_or_else(||"ASR_NOT_INITIALIZED".into())}
fn private_dir(path:&Path)->Result<(),String> {
    fs::create_dir_all(path).map_err(|e|e.to_string())?;
    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(path,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
    Ok(())
}
fn private_write(path:&Path,bytes:&[u8])->Result<(),String> {
    use std::io::Write;
    let temporary=path.with_extension(format!("{}.part",uuid::Uuid::new_v4()));
    let mut options=fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let mut file=options.open(&temporary).map_err(|e|e.to_string())?;
    file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
    fs::rename(&temporary,path).map_err(|e|e.to_string())
}
fn verify_snapshot(snapshot:&Path)->Result<String,String> {
    let bytes=fs::read(snapshot.join("manifest.json")).map_err(|e|format!("ASR_RESOURCE_MISSING: {e}"))?;
    let manifest:Manifest=serde_json::from_slice(&bytes).map_err(|_|"ASR_RESOURCE_INVALID: manifest")?;
    if manifest.plugin_revision.is_empty() || manifest.protocol!=PROTOCOL || manifest.service_version!=SERVICE_VERSION || manifest.whisperx_version!="3.8.6" || manifest.files.len()!=3 {return Err("ASR_RESOURCE_INVALID: release manifest mismatch".into());}
    for name in ["server.py","prepare.py","requirements.txt"] {
        let expected=manifest.files.get(name).ok_or("ASR_RESOURCE_INVALID: missing hash")?;
        let content=fs::read(snapshot.join(name)).map_err(|e|e.to_string())?;
        if format!("{:x}",Sha256::digest(&content))!=*expected {return Err(format!("ASR_RESOURCE_INVALID: {name} hash mismatch"));}
    }
    Ok(format!("{:x}",Sha256::digest(&bytes)))
}
fn installed_valid(root:&Path,installed:&Installed,hash:&str)->bool {
    if uuid::Uuid::parse_str(&installed.installation_id).is_err() || installed.snapshot_hash!=hash || installed.config.clone().normalized().is_err(){return false;}
    let dir=root.join(&installed.installation_id);
    venv_python(&dir).is_file() && fs::read(dir.join("config.json")).ok().and_then(|bytes|serde_json::from_slice::<Installed>(&bytes).ok()).is_some_and(|config|config.installation_id==installed.installation_id && config.snapshot_hash==installed.snapshot_hash && config.config==installed.config)
}
fn venv_python(dir:&Path)->PathBuf {dir.join(if cfg!(windows) {"venv/Scripts/python.exe"}else{"venv/bin/python"})}
/// Model caches are content-addressed, so installations share one: a failed or re-configured
/// install resumes instead of downloading several GB again. Earlier installs kept their own.
fn model_cache(root:&Path,dir:&Path)->PathBuf {let legacy=dir.join("cache");if legacy.is_dir(){legacy}else{root.join("models")}}
/// Installation directories other than `keep` were left by interrupted or superseded installs.
fn remove_abandoned_installations(root:&Path,keep:Option<&str>) {
    let Ok(entries)=fs::read_dir(root) else {return;};
    for entry in entries.flatten() {
        let name=entry.file_name();let Some(name)=name.to_str() else {continue;};
        if uuid::Uuid::parse_str(name).is_ok() && Some(name)!=keep && entry.path().is_dir() {let _=fs::remove_dir_all(entry.path());}
    }
}
/// The service log lives in its run directory, which is removed on failure; keep it for diagnosis.
fn preserve_service_log(run_dir:&Path,destination:&Path) {
    use std::io::Write;
    let Ok(log)=fs::read(run_dir.join("service.log")) else {return;};
    let mut options=fs::OpenOptions::new();options.create(true).append(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    if let Ok(mut file)=options.open(destination) {let _=writeln!(file,"\n==> service.log ({})",run_dir.display());let _=file.write_all(&log);}
}
fn stopped()->RuntimeStatus {RuntimeStatus {state:"stopped".into(),pid:None,active_task_id:None}}
pub fn initialize(app:&tauri::AppHandle)->Result<(),String> {
    let resources=crate::media_runtime::runtime::resource_directory(app)?;
    let python=crate::media_runtime::runtime::tools_at(&crate::media_runtime::runtime::root()?)?.remove("python").ok_or("ASR_PYTHON_MISSING: bundled Python is required")?;
    initialize_at(app.path().app_data_dir().map_err(|e|e.to_string())?,resources.join("dsivio-video-asr").join(SERVICE_VERSION),python)
}
fn initialize_at(data:PathBuf,snapshot:PathBuf,python:PathBuf)->Result<(),String> {
    let root=data.join("media-services/whisperx").join(SERVICE_VERSION);private_dir(&root)?;
    let evidence=data.join("media-tasks");private_dir(&evidence)?;
    let hash=verify_snapshot(&snapshot)?;
    let active=fs::read(root.join("active.json")).ok().and_then(|b|serde_json::from_slice::<Installed>(&b).ok()).filter(|i|installed_valid(&root,i,&hash));
    remove_abandoned_installations(&root,active.as_ref().map(|i|i.installation_id.as_str()));
    let config=active.as_ref().map(|i|i.config.clone()).unwrap_or_default();
    let status=LocalAsrStatus {operation_id:None,state:if active.is_some(){"ready"}else{"notInstalled"}.into(),installation_id:active.as_ref().map(|i|i.installation_id.clone()),service_version:SERVICE_VERSION.into(),model:config.model,languages:config.languages,progress:None,error:None,runtime:stopped()};
    let supervisor=Arc::new(Supervisor {root,snapshot,python,evidence,state:Mutex::new(InstallState {status,active,cancel:None}),service:Mutex::new(None),startup:Mutex::new(None),queue:Arc::new(Semaphore::new(1)),shutting_down:std::sync::atomic::AtomicBool::new(false),client:reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?});
    OWNER.set(supervisor.clone()).map_err(|_|"ASR_ALREADY_INITIALIZED")?;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut guard=supervisor.service.lock().await;
            if let Some(service)=guard.as_mut() {
                let exited=service.child.try_wait().ok().flatten().is_some();
                if exited || service.active_task_id.is_none() && service.last_used.elapsed()>=Duration::from_secs(300) {
                    if let Some(mut service)=guard.take() {let _=terminate(&supervisor.client,&mut service).await;}
                }
            }
        }
    });
    Ok(())
}
pub async fn status()->Result<LocalAsrStatus,String> {
    let owner=owner()?;let mut result=owner.state.lock().await.status.clone();
    let Ok(mut guard)=owner.service.try_lock() else {return Ok(result);};
    if let Some(service)=guard.as_mut() {
        if service.child.try_wait().map_err(|e|e.to_string())?.is_none() {result.runtime=RuntimeStatus {state:if service.active_task_id.is_some(){"busy"}else{"ready"}.into(),pid:service.child.id(),active_task_id:service.active_task_id.clone()};}
        else {guard.take();}
    }
    Ok(result)
}
pub async fn install(config:LocalAsrConfig)->Result<LocalAsrStatus,String> {
    let owner=owner()?;let config=config.normalized()?;let hash=verify_snapshot(&owner.snapshot)?;
    if owner.shutting_down.load(std::sync::atomic::Ordering::Acquire){return Err("ASR_SHUTDOWN".into());}
    let mut state=owner.state.lock().await;
    if state.status.state=="installing" {
        if state.status.model!=config.model || state.status.languages!=config.languages {return Err("ASR_INSTALL_CONFLICT: another installation configuration is active".into());}
        return Ok(state.status.clone());
    }
    if state.active.as_ref().is_some_and(|i|i.config.same_install(&config) && installed_valid(&owner.root,i,&hash)) {
        state.status.state="ready".into();state.status.model=config.model;state.status.languages=config.languages;state.status.error=None;state.status.progress=None;return Ok(state.status.clone());
    }
    let id=uuid::Uuid::new_v4().to_string();let (cancel,receiver)=watch::channel(false);
    state.status.operation_id=Some(id.clone());state.status.state="installing".into();state.status.model=config.model.clone();state.status.languages=config.languages.clone();state.status.error=None;
    state.status.progress=Some(LocalAsrProgress {stage:"starting".into(),message:"Preparing a new isolated WhisperX installation".into(),downloaded_bytes:None});state.cancel=Some(cancel);
    let result=state.status.clone();drop(state);
    tauri::async_runtime::spawn(async move {
        let installed=Installed {installation_id:id.clone(),config,snapshot_hash:hash};
        let outcome=install_staging(&owner,&installed,receiver).await;
        if outcome.is_err() {let _=fs::remove_dir_all(owner.root.join(&id));}
        let mut state=owner.state.lock().await;
        if state.status.operation_id.as_deref()!=Some(&id) {return;}
        state.cancel=None;
        match outcome {
            Ok(())=>{
                // Activation already stopped the old service; its environment is no longer used.
                if let Some(previous)=state.active.as_ref() {let _=fs::remove_dir_all(owner.root.join(&previous.installation_id));}
                state.status.state="ready".into();state.status.installation_id=Some(id);state.status.progress=None;state.active=Some(installed);
            }
            Err(error)=>{state.status.state="failed".into();state.status.error=Some(error);state.status.progress=None;}
        }
    });
    Ok(result)
}
pub async fn cancel_install(operation_id:&str)->Result<LocalAsrStatus,String> {
    let owner=owner()?;{
        let state=owner.state.lock().await;
        if state.status.operation_id.as_deref()!=Some(operation_id) {return Err("ASR_INSTALL_OPERATION_MISMATCH".into());}
        if let Some(cancel)=&state.cancel {let _=cancel.send(true);}
    }
    loop {let state=owner.state.lock().await;if state.status.state!="installing" {return Ok(state.status.clone());}drop(state);tokio::time::sleep(Duration::from_millis(50)).await;}
}
async fn progress(owner:&Supervisor,stage:&str,message:&str) {
    owner.state.lock().await.status.progress=Some(LocalAsrProgress {stage:stage.into(),message:message.into(),downloaded_bytes:None});
}
/// `pycache` keeps bytecode in App data: the venv shares the bundled stdlib, which must stay unmodified.
fn owned_command(program:&Path,pycache:&Path)->Command {
    let mut command=Command::new(program);command.kill_on_drop(true).env_clear().env("PYTHONPYCACHEPREFIX",pycache);
    for key in ["PATH","HOME","USERPROFILE","SYSTEMROOT","SystemRoot","WINDIR","TEMP","TMP","TMPDIR","LANG","LC_ALL","LC_CTYPE","SSL_CERT_FILE","SSL_CERT_DIR"] {
        if let Some(value)=std::env::var_os(key){command.env(key,value);}
    }
    #[cfg(unix)] {use std::os::unix::process::CommandExt;command.as_std_mut().process_group(0);}
    command
}
async fn kill_owned(child:&mut Child)->Result<(),String> {
    #[cfg(unix)] if let Some(pid)=child.id() {unsafe {libc::kill(-(pid as i32),libc::SIGKILL);}}
    let _=child.start_kill();child.wait().await.map(|_|()).map_err(|e|e.to_string())
}
/// Bytes on disk under `path`. Hugging Face snapshot links point into blobs, so links are not followed.
fn tree_size(path:&Path)->u64 {
    let Ok(metadata)=fs::symlink_metadata(path) else {return 0;};
    if !metadata.is_dir() {return if metadata.is_file() {metadata.len()} else {0};}
    fs::read_dir(path).map(|entries|entries.flatten().map(|entry|tree_size(&entry.path())).sum()).unwrap_or(0)
}
/// Runs one preparation step. It keeps going while the log or `watched` grows and reports the bytes
/// added to `watched`; it stops after `stall` without progress or the overall `limit`.
async fn run_install_command(mut command:Command,log:&Path,watched:&Path,cancel:&mut watch::Receiver<bool>,watchdog:&Watchdog,on_progress:&(dyn Fn(u64)+Sync))->Result<(),String> {
    use std::process::Stdio;
    let mut options=fs::OpenOptions::new();options.create(true).append(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let file=options.open(log).map_err(|e|e.to_string())?;
    command.stdout(Stdio::from(file.try_clone().map_err(|e|e.to_string())?)).stderr(Stdio::from(file)).stdin(Stdio::null());
    if *cancel.borrow() {return Err("ASR_INSTALL_CANCELLED".into());}
    let measure=|| {let (log,watched)=(log.to_owned(),watched.to_owned());tokio::task::spawn_blocking(move||(tree_size(&watched),tree_size(&log)))};
    let (baseline,_)=measure().await.map_err(|e|e.to_string())?;
    let mut child=command.spawn().map_err(|e|format!("ASR_INSTALL_FAILED: cannot launch bundled Python: {e}"))?;
    let started=std::time::Instant::now();let mut seen=None;let mut moved=started;
    let mut ticker=tokio::time::interval(watchdog.poll);ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            result=child.wait()=>{let status=result.map_err(|e|e.to_string())?;return if status.success(){Ok(())}else{Err(format!("ASR_INSTALL_FAILED: preparation exited {status}; private diagnostics: {}",log.display()))};},
            _=cancel.changed()=>{kill_owned(&mut child).await?;return Err("ASR_INSTALL_CANCELLED".into());},
            _=ticker.tick()=>{
                let (watched_size,log_size)=measure().await.map_err(|e|e.to_string())?;
                on_progress(watched_size.saturating_sub(baseline));
                if seen!=Some((watched_size,log_size)) {seen=Some((watched_size,log_size));moved=std::time::Instant::now();}
                else if moved.elapsed()>=watchdog.stall {
                    kill_owned(&mut child).await?;
                    return Err(format!("ASR_INSTALL_STALLED: no download progress for {} minutes; retrying resumes from the files already downloaded; private diagnostics: {}",watchdog.stall.as_secs()/60,log.display()));
                }
                if started.elapsed()>=watchdog.limit {kill_owned(&mut child).await?;return Err(format!("ASR_INSTALL_TIMEOUT: step exceeded {} hours; private diagnostics: {}",watchdog.limit.as_secs()/3600,log.display()));}
            },
        }
    }
}
fn prepare_command(python:&Path,pycache:&Path,snapshot:&Path,config:&LocalAsrConfig,cache:&Path)->Command {
    let mut command=owned_command(python,pycache);
    command.arg(snapshot.join("prepare.py")).arg("--model").arg(&config.model).arg("--languages").args(&config.languages).arg("--cache").arg(cache);
    // Xet keeps a file in memory until it is complete, so progress is invisible and an interrupted
    // download restarts from zero. Plain HTTP writes `.incomplete` files that later attempts resume.
    command.env("HF_HUB_DISABLE_XET","1");
    // For .bin-only alignment models transformers also fetches a converted safetensors copy (~1.3 GB)
    // in a background thread that keeps this step alive after it reports success. Inference reads .bin.
    command.env("DISABLE_SAFETENSORS_CONVERSION","true");
    command
}
async fn install_staging(owner:&Supervisor,installed:&Installed,mut cancel:watch::Receiver<bool>)->Result<(),String> {
    let dir=owner.root.join(&installed.installation_id);private_dir(&dir)?;let cache=owner.root.join("models");private_dir(&cache)?;
    let log=owner.root.join(format!("install-{}.log",installed.installation_id));
    progress(owner,"venv","Creating an isolated environment with bundled Python").await;
    let mut command=owned_command(&owner.python,&owner.root.join("pycache"));command.args(["-m","venv"]).arg(dir.join("venv"));
    run_install_command(command,&log,&dir,&mut cancel,&INSTALL_WATCHDOG,&|_|{}).await?;
    progress(owner,"dependencies","Installing pinned WhisperX dependencies").await;
    let python=venv_python(&dir);let mut command=owned_command(&python,&owner.root.join("pycache"));
    command.args(["-m","pip","install","--disable-pip-version-check","-r"]).arg(owner.snapshot.join("requirements.txt"));
    run_install_command(command,&log,&dir,&mut cancel,&INSTALL_WATCHDOG,&|_|{}).await?;
    progress(owner,"models","Downloading the ASR, alignment and NLTK model caches").await;
    let command=prepare_command(&python,&owner.root.join("pycache"),&owner.snapshot,&installed.config,&cache);
    let report=|bytes:u64| {if let Ok(mut state)=owner.state.try_lock() {if let Some(progress)=state.status.progress.as_mut().filter(|p|p.stage=="models") {progress.downloaded_bytes=Some(bytes);}}};
    run_install_command(command,&log,&cache,&mut cancel,&INSTALL_WATCHDOG,&report).await?;
    private_write(&dir.join("config.json"),&serde_json::to_vec(installed).map_err(|e|e.to_string())?)?;
    progress(owner,"selfTest","Verifying the offline service and real WAV inference").await;
    let probe=dir.join("self-test");private_dir(&probe)?;let audio=probe.join("silence.wav");
    private_write(&audio,&silence_wav())?;
    let mut service=start_service(owner,&dir,&installed.config,&probe,&log,&mut cancel).await?;
    let selftest=async {
        for language in &installed.config.languages {
            let response=owner.client.post(format!("http://127.0.0.1:{}/transcribe",service.port)).bearer_auth(&service.nonce)
                .json(&json!({"audio_path":audio,"language":language,"task_id":"installation-self-test","sample_frames":16000,"timestamps":"word"})).timeout(Duration::from_secs(300)).send().await.map_err(|e|format!("ASR_SELF_TEST_FAILED: {e}"))?;
            if !response.status().is_success(){
                let detail=response.text().await.unwrap_or_default();
                return Err(format!("ASR_SELF_TEST_FAILED: {language} offline inference refused standard evidence: {}",detail.chars().take(300).collect::<String>()));
            }
            let result:Value=response.json().await.map_err(|e|e.to_string())?;
            validate_transcript(&result,language,16000)?;
        }
        Ok::<(),String>(())
    };
    let result=tokio::select! {result=selftest=>result,_=cancel.changed()=>Err("ASR_INSTALL_CANCELLED".into())};
    if result.is_err() {preserve_service_log(&service.run_dir,&log);}
    terminate(&owner.client,&mut service).await?;result.map_err(|error|format!("{error}; private diagnostics: {}",log.display()))?;
    let _=fs::remove_dir_all(&probe);
    if *cancel.borrow(){return Err("ASR_INSTALL_CANCELLED".into());}
    if owner.startup.lock().await.is_some(){return Err("ASR_BUSY: a task is starting the service".into());}
    let mut current=owner.service.lock().await;
    if let Some(service)=current.as_mut() {
        if let Some(task)=&service.active_task_id {return Err(format!("ASR_BUSY: active task {task}; installation was not activated"));}
        terminate(&owner.client,service).await?;current.take();
    }
    // Cancellation and activation are serialized: after activation cancellation is too late.
    let state=owner.state.lock().await;
    if *cancel.borrow(){return Err("ASR_INSTALL_CANCELLED".into());}
    if verify_snapshot(&owner.snapshot)?!=installed.snapshot_hash{return Err("ASR_RESOURCE_CHANGED: release snapshot changed during installation".into());}
    private_write(&owner.root.join("active.json"),&serde_json::to_vec(installed).map_err(|e|e.to_string())?)?;
    drop(state);Ok(())
}
fn silence_wav()->Vec<u8> {
    let mut bytes=b"RIFF".to_vec();bytes.extend(32036u32.to_le_bytes());bytes.extend(b"WAVEfmt ");bytes.extend(16u32.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(16000u32.to_le_bytes());bytes.extend(32000u32.to_le_bytes());bytes.extend(2u16.to_le_bytes());bytes.extend(16u16.to_le_bytes());bytes.extend(b"data");bytes.extend(32000u32.to_le_bytes());bytes.resize(32044,0);bytes
}
async fn start_service(owner:&Supervisor,dir:&Path,config:&LocalAsrConfig,allow_root:&Path,diagnostics:&Path,cancel:&mut watch::Receiver<bool>)->Result<OwnedService,String> {
    verify_snapshot(&owner.snapshot)?;
    let generation=uuid::Uuid::new_v4().to_string();let run_dir=dir.join("run").join(&generation);private_dir(&run_dir)?;
    let nonce=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());private_write(&run_dir.join("nonce"),nonce.as_bytes())?;
    let mut options=fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let log=options.open(run_dir.join("service.log")).map_err(|e|e.to_string())?;
    let mut command=owned_command(&venv_python(dir),&owner.root.join("pycache"));
    command.arg("-u").arg(owner.snapshot.join("server.py")).args(["--port","0","--model"]).arg(&config.model)
        .args(["--device","cpu","--compute","int8","--batch-size","8","--cache"]).arg(model_cache(&owner.root,dir))
        .arg("--allow-root").arg(allow_root).arg("--token-file").arg(run_dir.join("nonce"))
        .arg("--parent-pid").arg(std::process::id().to_string()).arg("--supervised-stdin")
        .env("HF_HUB_OFFLINE","1").env("TRANSFORMERS_OFFLINE","1").env("HF_HUB_DISABLE_TELEMETRY","1")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::from(log));
    if *cancel.borrow(){return Err("ASR_SERVICE_CANCELLED".into());}
    let mut child=command.spawn().map_err(|e|format!("ASR_START_FAILED: {e}"))?;
    let startup=async {
        let stdout=child.stdout.take().ok_or("ASR_START_FAILED: stdout missing")?;
        let mut reader=BufReader::new(stdout);
        let ready=loop {
            let mut line=Vec::new();
            let read=reader.read_until(b'\n',&mut line).await.map_err(|e|e.to_string())?;
            if read==0 {return Err("ASR_START_FAILED: child exited before ready".into());}
            if read>16*1024 {return Err("ASR_START_FAILED: excessive ready line".into());}
            if let Ok(value)=serde_json::from_slice::<Value>(&line) {if value.get("port").is_some(){break value;}}
        };
        let port=ready["port"].as_u64().and_then(|n|u16::try_from(n).ok()).filter(|n|*n>0).ok_or("ASR_READY_INVALID: port")?;
        if ready["pid"].as_u64()!=child.id().map(u64::from) || ready["protocol"]!=PROTOCOL || ready["serviceVersion"]!=SERVICE_VERSION {return Err("ASR_READY_INVALID: owned child identity mismatch".into());}
        let response=owner.client.get(format!("http://127.0.0.1:{port}/health")).bearer_auth(&nonce).timeout(Duration::from_secs(10)).send().await.map_err(|e|format!("ASR_HEALTH_FAILED: {e}"))?;
        if !response.status().is_success(){return Err("ASR_HEALTH_FAILED: service refused owner".into());}
        let health:Value=response.json().await.map_err(|_|"ASR_HEALTH_INVALID")?;
        if health["ok"]!=true || health["protocol"]!=PROTOCOL || health["serviceVersion"]!=SERVICE_VERSION || health["whisperxVersion"]!="3.8.6" || health["model"]!=config.model || health["device"]!="cpu" || health["compute"]!="int8" || health["batchSize"]!=8 || health["busy"]!=false {return Err("ASR_HEALTH_INVALID: installed configuration mismatch".into());}
        Ok::<u16,String>(port)
    };
    let result=tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(120),startup)=>result.unwrap_or_else(|_|Err("ASR_START_TIMEOUT".into())),
        _=cancel.changed()=>Err("ASR_SERVICE_CANCELLED".into()),
    };
    let port=match result {Ok(port)=>port,Err(error)=>{
        let _=kill_owned(&mut child).await;preserve_service_log(&run_dir,diagnostics);let _=fs::remove_dir_all(&run_dir);
        return Err(format!("{error}; private diagnostics: {}",diagnostics.display()));
    }};
    private_write(&run_dir.join("run.json"),&serde_json::to_vec(&json!({"generation":generation,"port":port,"pid":child.id(),"protocol":PROTOCOL,"serviceVersion":SERVICE_VERSION})).map_err(|e|e.to_string())?)?;
    Ok(OwnedService {child,port,nonce,generation,active_task_id:None,last_used:std::time::Instant::now(),run_dir,installation_dir:dir.into()})
}
async fn terminate(client:&reqwest::Client,service:&mut OwnedService)->Result<(),String> {
    if service.child.try_wait().map_err(|e|e.to_string())?.is_none() {
        if service.active_task_id.is_none() {
            let _=client.post(format!("http://127.0.0.1:{}/shutdown",service.port)).bearer_auth(&service.nonce).json(&json!({})).timeout(Duration::from_secs(2)).send().await;
        }
        // Closing the supervised stdin is a second, non-network owner shutdown signal.
        service.child.stdin.take();
        match tokio::time::timeout(Duration::from_secs(3),service.child.wait()).await {
            Ok(result)=>{result.map_err(|e|e.to_string())?;},
            Err(_)=>kill_owned(&mut service.child).await?,
        }
    }
    let _=fs::remove_dir_all(&service.run_dir);Ok(())
}
pub async fn stop()->Result<LocalAsrStopResult,String> {
    let owner=owner()?;
    if let Some((task,_,_))=owner.startup.lock().await.as_ref(){return Err(format!("ASR_BUSY: active task {task} is starting"));}
    let mut guard=owner.service.lock().await;
    if let Some(service)=guard.as_mut() {
        if let Some(task)=&service.active_task_id {return Err(format!("ASR_BUSY: active task {task}; cancel the task first"));}
        terminate(&owner.client,service).await?;guard.take();
    }
    Ok(LocalAsrStopResult {outcome:"confirmed".into(),runtime:stopped()})
}
pub async fn shutdown() {
    if let Ok(owner)=owner() {
        owner.shutting_down.store(true,std::sync::atomic::Ordering::Release);owner.queue.close();
        if let Some((_,_,cancel))=owner.startup.lock().await.as_ref(){let _=cancel.send(true);}
        let receiver={let state=owner.state.lock().await;state.cancel.as_ref().map(|sender|{let receiver=sender.subscribe();let _=sender.send(true);receiver})};
        if receiver.is_some() {
            loop {if owner.state.lock().await.cancel.is_none(){break;}tokio::time::sleep(Duration::from_millis(50)).await;}
        }
        let mut guard=owner.service.lock().await;
        if let Some(mut service)=guard.take(){let _=terminate(&owner.client,&mut service).await;}
    }
}
pub async fn cancel(task_id:&str)->Result<bool,String> {
    let owner=owner()?;
    let cancelled_startup={let startup=owner.startup.lock().await;if let Some((_,_,cancel))=startup.as_ref().filter(|(task,_,_)|task==task_id){let _=cancel.send(true);true}else{false}};
    let mut guard=owner.service.lock().await;
    let Some(service)=guard.as_mut() else {return Ok(cancelled_startup);};
    if service.active_task_id.as_deref()!=Some(task_id) {return Ok(false);}
    // Only the child handle we created can be stopped. Disk PIDs are never trusted.
    let _=owner.client.post(format!("http://127.0.0.1:{}/cancel",service.port)).bearer_auth(&service.nonce).json(&json!({"task_id":task_id})).timeout(Duration::from_secs(2)).send().await;
    terminate(&owner.client,service).await?;guard.take();Ok(true)
}
struct StartupGuard(watch::Sender<bool>);
impl Drop for StartupGuard {fn drop(&mut self){let _=self.0.send(true);}}
struct TaskLease {owner:Arc<Supervisor>,generation:Option<String>,permit:Option<tokio::sync::OwnedSemaphorePermit>,armed:bool}
impl Drop for TaskLease {
    fn drop(&mut self) {
        if !self.armed {return;}
        let owner=self.owner.clone();let generation=self.generation.clone();let permit=self.permit.take();
        tauri::async_runtime::spawn(async move {
            // Keep the next queued task out until this generation has actually exited.
            let _permit=permit;let mut guard=owner.service.lock().await;
            if generation.as_ref().is_some_and(|generation|guard.as_ref().is_some_and(|s|&s.generation==generation)) {
                if let Some(mut service)=guard.take(){let _=terminate(&owner.client,&mut service).await;}
            }
        });
    }
}
pub async fn transcribe(task_id:&str,audio:&Path,language:&str,sample_frames:u64,timestamps:&str,config:LocalAsrConfig)->Result<Value,String> {
    let owner=owner()?;let config=config.normalized()?;super::transcribe_providers::validate_language(language)?;
    if !config.languages.iter().any(|l|l==language){return Err("ASR_LANGUAGE_NOT_INSTALLED: add the language in local ASR settings".into());}
    if !matches!(timestamps,"word"|"segment"){return Err("ASR_TIMESTAMPS_INVALID".into());}
    super::transcribe_providers::validate_wav_frames(audio,Some(sample_frames))?;
    let queued=owner.queue.clone().acquire_owned().await.map_err(|_|"ASR_SHUTDOWN")?;
    let active={owner.state.lock().await.active.clone()};
    let ready=active.as_ref().is_some_and(|i|i.config.same_install(&config) && verify_snapshot(&owner.snapshot).is_ok_and(|hash|installed_valid(&owner.root,i,&hash)));
    if !ready {
        if !config.auto_install {return Err("ASR_INSTALL_REQUIRED: Settings > Media Creation or dsivio media asr install".into());}
        let operation=install(config.clone()).await?.operation_id;
        loop {
            let state=owner.state.lock().await;
            if state.status.operation_id!=operation {return Err("ASR_INSTALL_CONFLICT".into());}
            if state.status.state=="failed" {return Err(state.status.error.clone().unwrap_or_else(||"ASR_INSTALL_FAILED".into()));}
            if state.status.state=="ready" {break;}
            drop(state);tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let installed=owner.state.lock().await.active.clone().ok_or("ASR_INSTALL_REQUIRED")?;
    let audio=fs::canonicalize(audio).map_err(|e|e.to_string())?;
    let evidence=fs::canonicalize(&owner.evidence).map_err(|e|e.to_string())?;
    if !audio.starts_with(&evidence){return Err("ASR_AUDIO_OUTSIDE_EVIDENCE_ROOT: task owner must snapshot the WAV before acceptance".into());}
    let (sender,mut receiver)=watch::channel(false);
    let startup_guard=StartupGuard(sender.clone());
    let task=task_id.to_owned();let start_owner=owner.clone();
    let startup_generation=uuid::Uuid::new_v4().to_string();
    let runtime=tauri::async_runtime::spawn(async move {
        let mut lease=TaskLease {owner:start_owner.clone(),generation:None,permit:Some(queued),armed:true};
        if start_owner.shutting_down.load(std::sync::atomic::Ordering::Acquire){return Err("ASR_SHUTDOWN".into());}
        *start_owner.startup.lock().await=Some((task.clone(),startup_generation.clone(),sender));
        start_owner.state.lock().await.status.runtime=RuntimeStatus {state:"starting".into(),pid:None,active_task_id:Some(task.clone())};
        let result=async {
            let mut guard=start_owner.service.lock().await;
            if guard.as_mut().is_some_and(|s|s.child.try_wait().ok().flatten().is_some()){guard.take();}
            let installation_dir=start_owner.root.join(&installed.installation_id);
            if guard.as_ref().is_some_and(|service|service.installation_dir!=installation_dir){if let Some(mut service)=guard.take(){terminate(&start_owner.client,&mut service).await?;}}
            if guard.is_none(){*guard=Some(start_service(&start_owner,&start_owner.root.join(&installed.installation_id),&config,&evidence,&start_owner.root.join("service-failures.log"),&mut receiver).await?);}
            let service=guard.as_mut().unwrap();service.active_task_id=Some(task);
            lease.generation=Some(service.generation.clone());
            if *receiver.borrow(){return Err("ASR_SERVICE_CANCELLED".into());}
            Ok::<_,String>((service.port,service.nonce.clone(),service.generation.clone()))
        }.await;
        let mut startup=start_owner.startup.lock().await;
        if startup.as_ref().is_some_and(|(_,generation,_)|generation==&startup_generation){startup.take();start_owner.state.lock().await.status.runtime=stopped();}
        drop(startup);
        result.map(|(port,nonce,generation)|(port,nonce,generation,lease))
    });
    let (port,nonce,generation,mut lease)=runtime.await.map_err(|e|e.to_string())??;
    // This sender is no longer connected after startup; no child can be cancelled by it.
    drop(startup_guard);
    let result=async {
        let mut response=owner.client.post(format!("http://127.0.0.1:{port}/transcribe")).bearer_auth(&nonce)
            .json(&json!({"audio_path":audio,"language":language,"task_id":task_id,"sample_frames":sample_frames,"timestamps":timestamps}))
            .timeout(Duration::from_secs(1800)).send().await.map_err(|_|"ASR_SERVICE_FAILED: owned inference connection ended")?;
        if !response.status().is_success(){return Err(format!("ASR_SERVICE_FAILED: HTTP {}",response.status()));}
        let mut bytes=Vec::new();
        while let Some(chunk)=response.chunk().await.map_err(|_|"ASR_SERVICE_FAILED: interrupted reply")? {
            if bytes.len()+chunk.len()>32*1024*1024 {return Err("ASR_REPLY_TOO_LARGE".into());}
            bytes.extend_from_slice(&chunk);
        }
        let mut transcript:Value=serde_json::from_slice(&bytes).map_err(|_|"ASR_REPLY_INVALID")?;
        validate_transcript(&transcript,language,sample_frames)?;
        if timestamps=="segment" {for segment in transcript["segments"].as_array_mut().unwrap(){segment.as_object_mut().unwrap().remove("words");}}
        Ok::<Value,String>(transcript)
    }.await;
    let mut guard=owner.service.lock().await;
    if let Some(service)=guard.as_mut().filter(|s|s.generation==generation) {
        if result.is_ok() {
            if service.child.try_wait().map_err(|e|e.to_string())?.is_some(){guard.take();return Err("ASR_SERVICE_EXITED: inference child terminated".into());}
            service.active_task_id=None;service.last_used=std::time::Instant::now();
        }else{terminate(&owner.client,service).await?;guard.take();}
    }else {return Err("ASR_SERVICE_CANCELLED: owned child was stopped".into());}
    lease.armed=false;result
}
fn validate_transcript(transcript:&Value,language:&str,frames:u64)->Result<(),String> {
    if transcript["schema"]!="dsivio.media.transcript/1" || transcript["language"]!=language || transcript["sampleRate"]!=16000 || transcript["sampleFrames"]!=frames ||
        transcript["engine"]["backend"]!="local" || transcript["engine"]["model"]!="small" || transcript["engine"]["protocol"]!=PROTOCOL || transcript["engine"]["serviceVersion"]!=SERVICE_VERSION || transcript["engine"]["whisperxVersion"]!="3.8.6" || !transcript["segments"].is_array() {
        return Err("ASR_REPLY_INVALID: transcript identity/evidence mismatch".into());
    }
    for segment in transcript["segments"].as_array().unwrap() {
        if !segment["text"].is_string() {return Err("ASR_REPLY_INVALID: segment text".into());}
        let mut values=vec![segment];
        if let Some(words)=segment.get("words") {values.extend(words.as_array().ok_or("ASR_REPLY_INVALID: words")?);}
        for value in values {
            if !value["text"].is_string() {return Err("ASR_REPLY_INVALID: word text".into());}
            for field in ["start","end","score"] {if let Some(n)=value.get(field){if !n.as_f64().is_some_and(|n|n.is_finite() && n>=0.0){return Err("ASR_REPLY_INVALID: timing/score".into());}}}
            if value.get("start").and_then(Value::as_f64).zip(value.get("end").and_then(Value::as_f64)).is_some_and(|(s,e)|e<s){return Err("ASR_REPLY_INVALID: reversed timing".into());}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_configuration_is_order_independent_but_language_specific() {
        let mut config=LocalAsrConfig::default();config.languages=vec!["zh".into(),"en".into(),"zh".into()];
        assert!(config.normalized().unwrap().same_install(&LocalAsrConfig::default()));
        assert!(!LocalAsrConfig {languages:vec!["en".into()],..LocalAsrConfig::default()}.same_install(&LocalAsrConfig::default()));
        assert!(LocalAsrConfig {languages:vec!["auto".into()],..LocalAsrConfig::default()}.normalized().is_err());
    }
    #[test]
    fn snapshot_tampering_is_rejected_before_python_execution() {
        let root=std::env::temp_dir().join(format!("dsivio-asr-hash-{}",uuid::Uuid::new_v4()));private_dir(&root).unwrap();
        let mut files=BTreeMap::new();
        for name in ["server.py","prepare.py","requirements.txt"] {let bytes=name.as_bytes();private_write(&root.join(name),bytes).unwrap();files.insert(name,format!("{:x}",Sha256::digest(bytes)));}
        private_write(&root.join("manifest.json"),&serde_json::to_vec(&json!({"pluginRevision":"release-fixture","protocol":PROTOCOL,"serviceVersion":SERVICE_VERSION,"whisperxVersion":"3.8.6","files":files})).unwrap()).unwrap();
        let before=verify_snapshot(&root).unwrap();
        private_write(&root.join("server.py"),b"changed source").unwrap();
        assert!(verify_snapshot(&root).unwrap_err().contains("server.py hash mismatch"));
        private_write(&root.join("server.py"),b"server.py").unwrap();assert_eq!(verify_snapshot(&root).unwrap(),before);
        fs::remove_dir_all(root).unwrap();
    }
    fn scratch(name:&str)->PathBuf {let root=std::env::temp_dir().join(format!("dsivio-asr-{name}-{}",uuid::Uuid::new_v4()));private_dir(&root).unwrap();root}
    #[test]
    fn installations_share_one_model_cache_so_retries_resume() {
        let root=scratch("cache");let fresh=root.join(uuid::Uuid::new_v4().to_string());private_dir(&fresh).unwrap();
        assert_eq!(model_cache(&root,&fresh),root.join("models"));
        // Installations made before the shared cache keep reading their own.
        let legacy=root.join(uuid::Uuid::new_v4().to_string());private_dir(&legacy.join("cache")).unwrap();
        assert_eq!(model_cache(&root,&legacy),legacy.join("cache"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn abandoned_installations_are_removed_but_shared_state_is_kept() {
        let root=scratch("abandoned");let active=uuid::Uuid::new_v4().to_string();let orphan=uuid::Uuid::new_v4().to_string();
        for dir in [&active,&orphan,&"models".to_owned(),&"pycache".to_owned()] {private_dir(&root.join(dir)).unwrap();}
        private_write(&root.join(format!("install-{orphan}.log")),b"why it failed").unwrap();
        remove_abandoned_installations(&root,Some(&active));
        assert!(root.join(&active).is_dir() && root.join("models").is_dir() && root.join("pycache").is_dir());
        assert!(!root.join(&orphan).exists());
        assert!(root.join(format!("install-{orphan}.log")).is_file());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_service_log_outlives_its_run_directory() {
        let root=scratch("log");let run=root.join("run");private_dir(&run).unwrap();
        private_write(&run.join("service.log"),b"Traceback: RESOURCE_NOT_PREPARED").unwrap();
        let destination=root.join("install.log");private_write(&destination,b"pip output\n").unwrap();
        preserve_service_log(&run,&destination);fs::remove_dir_all(&run).unwrap();
        let kept=String::from_utf8(fs::read(&destination).unwrap()).unwrap();
        assert!(kept.starts_with("pip output") && kept.contains("RESOURCE_NOT_PREPARED"),"{kept}");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tree_size_counts_files_without_following_snapshot_links() {
        let root=scratch("size");private_dir(&root.join("blobs")).unwrap();
        private_write(&root.join("blobs/model"),&[0u8;1000]).unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink(root.join("blobs/model"),root.join("link")).unwrap();
        assert!((1000..1100).contains(&tree_size(&root)),"{}",tree_size(&root));
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn install_steps_wait_while_downloading_but_stop_when_stalled() {
        let root=scratch("watchdog");let log=root.join("install.log");let watched=root.join("models");private_dir(&watched).unwrap();
        let watchdog=Watchdog {poll:Duration::from_millis(50),stall:Duration::from_millis(400),limit:Duration::from_secs(30)};
        let (_cancel,mut receiver)=watch::channel(false);
        // A slow download that keeps writing outlives the stall window.
        let mut growing=Command::new("sh");growing.arg("-c").arg(format!("for i in 1 2 3 4 5 6 7 8 9 10 11 12; do head -c 4096 /dev/zero >> '{}/blob.incomplete'; sleep 0.1; done",watched.display()));
        let downloaded=std::sync::atomic::AtomicU64::new(0);
        run_install_command(growing,&log,&watched,&mut receiver,&watchdog,&|bytes|downloaded.store(bytes,std::sync::atomic::Ordering::Relaxed)).await.unwrap();
        assert!(downloaded.load(std::sync::atomic::Ordering::Relaxed)>0);
        // No output and no new bytes: stopped long before the hard limit.
        let mut stalled=Command::new("sleep");stalled.arg("20");
        let started=std::time::Instant::now();
        let error=run_install_command(stalled,&log,&watched,&mut receiver,&watchdog,&|_|{}).await.unwrap_err();
        assert!(error.starts_with("ASR_INSTALL_STALLED") && started.elapsed()<Duration::from_secs(5),"{error}");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn model_preparation_downloads_each_model_once_and_resumably() {
        let config=LocalAsrConfig {languages:vec!["en".into(),"pt".into()],..LocalAsrConfig::default()};
        let command=prepare_command(Path::new("python"),Path::new("/pycache"),Path::new("/snapshot"),&config,Path::new("/models"));
        let env=|key:&str|command.as_std().get_envs().find(|(k,_)|*k==key).and_then(|(_,v)|v).map(|v|v.to_string_lossy().into_owned());
        // transformers otherwise fetches a second ~1.3 GB safetensors copy per language in a thread
        // that keeps the step alive after preparation reports success.
        assert_eq!(env("DISABLE_SAFETENSORS_CONVERSION").as_deref(),Some("true"));
        assert_eq!(env("HF_HUB_DISABLE_XET").as_deref(),Some("1"));
        let args:Vec<_>=command.as_std().get_args().map(|a|a.to_string_lossy().into_owned()).collect();
        assert_eq!(args,["/snapshot/prepare.py","--model","small","--languages","en","pt","--cache","/models"]);
    }
    #[test]
    fn python_bytecode_stays_out_of_the_bundled_runtime() {
        // Bundled Python shares its stdlib with the venv; without a prefix every import writes
        // .pyc into App resources (signed bundle in release, a watched dev source tree in dev).
        let command=owned_command(Path::new("python"),Path::new("/data/whisperx/pycache"));
        let prefix=command.as_std().get_envs().find(|(key,_)|*key=="PYTHONPYCACHEPREFIX").and_then(|(_,value)|value);
        assert_eq!(prefix,Some(std::ffi::OsStr::new("/data/whisperx/pycache")));
    }
    #[test]
    fn unaligned_local_words_keep_absent_timing() {
        let transcript=json!({"schema":"dsivio.media.transcript/1","language":"zh","sampleRate":16000,"sampleFrames":16000,"engine":{"backend":"local","model":"small","protocol":PROTOCOL,"serviceVersion":SERVICE_VERSION,"whisperxVersion":"3.8.6"},"segments":[{"text":"今天","start":0.1,"end":0.8,"words":[{"text":"今","start":0.1,"end":0.3},{"text":"天"}]}]});
        validate_transcript(&transcript,"zh",16000).unwrap();
        assert!(validate_transcript(&transcript,"zh",16001).is_err());
        let mut reversed=transcript;reversed["segments"][0]["words"][0]["end"]=json!(0.05);assert!(validate_transcript(&reversed,"zh",16000).is_err());
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_worker_retains_queue_until_cleanup_finishes() {
        let config=LocalAsrConfig::default();
        let owner=Arc::new(Supervisor {
            root:PathBuf::new(),snapshot:PathBuf::new(),python:PathBuf::new(),evidence:PathBuf::new(),
            state:Mutex::new(InstallState {status:LocalAsrStatus {operation_id:None,state:"notInstalled".into(),installation_id:None,service_version:SERVICE_VERSION.into(),model:config.model,languages:config.languages,progress:None,error:None,runtime:stopped()},active:None,cancel:None}),
            service:Mutex::new(None),startup:Mutex::new(None),queue:Arc::new(Semaphore::new(1)),shutting_down:std::sync::atomic::AtomicBool::new(false),client:reqwest::Client::new(),
        });
        let cleanup_barrier=owner.service.lock().await;
        let permit=owner.queue.clone().acquire_owned().await.unwrap();
        drop(TaskLease {owner:owner.clone(),generation:None,permit:Some(permit),armed:true});
        // A dropped caller must not let its successor pass a still-pending cleanup.
        assert!(owner.queue.clone().try_acquire_owned().is_err());
        drop(cleanup_barrier);
        let next=tokio::time::timeout(Duration::from_secs(2),owner.queue.clone().acquire_owned()).await.unwrap().unwrap();
        drop(next);
    }
}
