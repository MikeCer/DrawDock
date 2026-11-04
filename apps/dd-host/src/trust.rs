use rcgen::generate_simple_self_signed;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
    Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
};

const MAX_DEVICES: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    token_hash: String,
}

#[derive(Serialize)]
pub struct DeviceCredential {
    pub id: String,
    pub token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    certificate: Vec<u8>,
    private_key: Vec<u8>,
    devices: Vec<TrustedDevice>,
}

pub struct TrustStore {
    path: PathBuf,
    state: State,
    _lock: File,
}

impl TrustStore {
    pub fn open_default() -> Result<Self, String> {
        let directory = std::env::var_os("LOCALAPPDATA")
            .ok_or("LOCALAPPDATA is unavailable; cannot protect host identity")?;
        Self::open(Path::new(&directory).join("DrawDock"))
    }

    fn open(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory)
            .map_err(|error| format!("create trust directory: {error}"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(directory.join("trust.lock"))
            .map_err(|error| {
                format!("lock trusted devices (stop other DrawDock hosts before managing trust): {error}")
            })?;
        let path = directory.join("trust.dpapi");
        let state = match fs::metadata(&path) {
            Ok(metadata) => {
                if metadata.len() > 1024 * 1024 {
                    return Err("encrypted trust store is too large".into());
                }
                let ciphertext =
                    fs::read(&path).map_err(|error| format!("read trust store: {error}"))?;
                let plaintext = protect(&ciphertext, false)?;
                let state: State = serde_json::from_slice(&plaintext)
                    .map_err(|_| "invalid decrypted trust store; refusing to replace identity")?;
                if state.version != 1
                    || state.devices.len() > MAX_DEVICES
                    || !(1..=16_384).contains(&state.certificate.len())
                    || !(1..=16_384).contains(&state.private_key.len())
                    || state.devices.iter().any(|device| {
                        !is_hex(&device.id, 32)
                            || !is_hex(&device.token_hash, 64)
                            || device.name.len() > 64
                            || device.name.chars().any(char::is_control)
                    })
                {
                    return Err("invalid trust store; refusing to replace identity".into());
                }
                state
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let identity = generate_simple_self_signed(vec!["drawdock".to_owned()])
                    .map_err(|error| format!("create persistent host TLS identity: {error}"))?;
                State {
                    version: 1,
                    certificate: identity.cert.der().to_vec(),
                    private_key: identity.key_pair.serialize_der(),
                    devices: Vec::new(),
                }
            }
            Err(error) => return Err(format!("inspect trust store: {error}")),
        };
        let store = Self {
            path,
            state,
            _lock: lock,
        };
        store.tls_config()?;
        if !store.path.exists() {
            store.save()?;
        }
        Ok(store)
    }

    pub fn fingerprint(&self) -> String {
        hex(&Sha256::digest(&self.state.certificate))
    }

    pub fn tls_config(&self) -> Result<ServerConfig, String> {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.state.certificate.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.state.private_key.clone())),
            )
            .map_err(|error| format!("configure persistent host TLS identity: {error}"))
    }

    pub fn devices(&self) -> &[TrustedDevice] {
        &self.state.devices
    }

    pub fn remember(&mut self, name: String) -> Result<DeviceCredential, String> {
        if self.state.devices.len() >= MAX_DEVICES {
            return Err("trusted-device limit reached; revoke an unused device first".into());
        }
        if name.len() > 64 || name.chars().any(char::is_control) {
            return Err("invalid trusted-device name".into());
        }
        let credential = DeviceCredential {
            id: random_hex::<16>()?,
            token: random_hex::<32>()?,
        };
        self.state.devices.push(TrustedDevice {
            id: credential.id.clone(),
            name,
            token_hash: hex(&Sha256::digest(credential.token.as_bytes())),
        });
        if let Err(error) = self.save() {
            self.state.devices.pop();
            return Err(error);
        }
        Ok(credential)
    }

    pub fn authenticate(&self, id: &str, token: &str) -> Option<&TrustedDevice> {
        if !is_hex(id, 32) || !is_hex(token, 64) {
            return None;
        }
        let digest = hex(&Sha256::digest(token.as_bytes()));
        self.state.devices.iter().find(|device| {
            device.id == id
                && crate::session::constant_time_eq(digest.as_bytes(), device.token_hash.as_bytes())
        })
    }

    pub fn revoke(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .state
            .devices
            .iter()
            .position(|device| device.id == id)
            .ok_or("unknown remembered-device ID")?;
        let device = self.state.devices.remove(index);
        if let Err(error) = self.save() {
            self.state.devices.insert(index, device);
            return Err(error);
        }
        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|error| format!("serialize protected trust store: {error}"))?;
        let encrypted = protect(&bytes, true)?;
        let temporary = self.path.with_extension("dpapi.new");
        let mut file = File::create(&temporary)
            .map_err(|error| format!("create encrypted trust update: {error}"))?;
        file.write_all(&encrypted)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("write encrypted trust update: {error}"))?;
        drop(file);
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Both paths are stable, NUL-terminated buffers; the exclusive lock covers replacement.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "replace encrypted trust store: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

fn protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| "trust data is too large")?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let purpose = b"DrawDock trusted devices v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: purpose.len() as u32,
        pbData: purpose.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI reads these buffers synchronously; its output must be released with LocalFree.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(format!(
            "{} current-user DPAPI trust store: {}",
            if encrypt { "protect" } else { "unlock" },
            io::Error::last_os_error()
        ));
    }
    if output.pbData.is_null() || output.cbData == 0 {
        unsafe {
            LocalFree(output.pbData.cast());
        }
        return Err("DPAPI returned an empty trust store".into());
    }
    // A successful DPAPI call owns an initialized buffer of cbData bytes.
    let plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(plaintext)
}

pub fn random_hex<const N: usize>() -> Result<String, String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("generate authentication credential: {error}"))?;
    Ok(hex(&bytes))
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Runs the built native host with an isolated trust store; requires an interactive desktop"]
    fn native_host_resumes_repeatedly_after_restart_and_rejects_revocation() {
        use crate::windows_overlay::Output;
        use rustls::{
            ClientConfig, ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName,
        };
        use std::{
            io::{BufRead, BufReader},
            net::{TcpListener, TcpStream},
            process::{Child, Command, Stdio},
            sync::{Arc, mpsc},
            thread,
            time::Duration,
        };
        struct Host {
            child: Child,
            events: mpsc::Receiver<String>,
            logger: Option<thread::JoinHandle<()>>,
        }
        impl Host {
            fn wait(&self, marker: &str) {
                loop {
                    let line = self
                        .events
                        .recv_timeout(Duration::from_secs(15))
                        .expect("Host did not become ready; inspect its stderr");
                    eprintln!("Test host: {line}");
                    if line.contains(marker) {
                        return;
                    }
                }
            }
        }
        impl Drop for Host {
            fn drop(&mut self) {
                if self.child.try_wait().ok().flatten().is_none() {
                    self.child.kill().expect("stop owned test host");
                }
                self.child.wait().expect("wait for owned test host");
                self.logger.take().unwrap().join().unwrap();
            }
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::temp_dir().join(format!(
            "drawdock-resume-test-{}",
            random_hex::<8>().unwrap()
        ));
        let mut trust = TrustStore::open(directory.join("DrawDock")).unwrap();
        let credential = trust.remember("TLS reconnect fixture".into()).unwrap();
        trust
            .remember("Independent retained device".into())
            .unwrap();
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(trust.state.certificate.clone()))
            .unwrap();
        let config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        drop(trust);
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("dd-host.exe");
        assert!(
            executable.is_file(),
            "Run cargo build -p dd-host before this test"
        );
        let output = Output::enumerate()
            .unwrap()
            .into_iter()
            .find(|output| output.primary)
            .unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let start = || {
            let mut child = Command::new(&executable)
                .args([
                    "serve",
                    "--output",
                    &output.name,
                    "--port",
                    &port.to_string(),
                ])
                .env("LOCALAPPDATA", &directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, events) = mpsc::channel();
            let logger = thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let line = line.expect("read native host output");
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            let host = Host {
                child,
                events,
                logger: Some(logger),
            };
            host.wait("Authenticated TLS/TCP listener");
            host
        };
        let resume = |token: &str| {
            let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
            let tls = ClientConnection::new(
                Arc::clone(&config),
                ServerName::try_from("drawdock").unwrap(),
            )
            .unwrap();
            let mut stream = BufReader::new(StreamOwned::new(tls, tcp));
            crate::session::write_json_line(
                stream.get_mut(),
                &serde_json::json!({
                    "type": "resume", "device_id": credential.id, "token": token,
                }),
            )
            .unwrap();
            let response = crate::session::read_json_line(&mut stream).unwrap();
            (stream, response)
        };
        let host = start();
        let (bad, rejection) = resume(&"0".repeat(64));
        assert_eq!(rejection["type"], "authentication_failed");
        drop(bad);
        let (stream, paired) = resume(&credential.token);
        assert_eq!(paired["type"], "paired");
        assert!(paired["remembered_device"].is_null());
        let first_video_token = paired["video_token"].clone();
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        let (stream, paired) = resume(&credential.token);
        assert_eq!(paired["type"], "paired");
        assert!(
            paired["video_token"] != first_video_token,
            "Each session needs a fresh video credential"
        );
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        drop(host);
        let host = start();
        let (stream, paired) = resume(&credential.token);
        assert_eq!(
            paired["type"], "paired",
            "Pinned TLS and trust must survive process restart"
        );
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        drop(host);
        let mut trust = TrustStore::open(directory.join("DrawDock")).unwrap();
        trust.revoke(&credential.id).unwrap();
        drop(trust);
        let host = start();
        let (stream, response) = resume(&credential.token);
        assert_eq!(response["type"], "authentication_failed");
        drop(stream);
        drop(host);
        fs::remove_dir_all(directory).unwrap();
        println!(
            "Native host reused pinned TLS trust across repeated sessions and process restart; wrong and revoked credentials rejected."
        );
    }

    #[test]
    fn dpapi_identity_credentials_and_revocation_survive_restart() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::temp_dir().join(format!(
            "drawdock-trust-test-{}",
            random_hex::<8>().unwrap()
        ));
        let mut store = TrustStore::open(directory.clone()).unwrap();
        let fingerprint = store.fingerprint();
        let credential = store.remember("Test tablet".into()).unwrap();
        assert!(
            TrustStore::open(directory.clone()).is_err(),
            "Concurrent writers must be excluded"
        );
        assert!(
            store
                .authenticate(&credential.id, &"0".repeat(64))
                .is_none()
        );
        assert!(store.authenticate("unknown", &credential.token).is_none());
        let encrypted = fs::read(&store.path).unwrap();
        assert!(
            !encrypted
                .windows(credential.token.len())
                .any(|part| part == credential.token.as_bytes())
        );
        drop(store);
        let mut store = TrustStore::open(directory.clone()).unwrap();
        assert_eq!(store.fingerprint(), fingerprint);
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_some()
        );
        store.revoke(&credential.id).unwrap();
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_none()
        );
        drop(store);
        let store = TrustStore::open(directory.clone()).unwrap();
        assert_eq!(store.fingerprint(), fingerprint);
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_none()
        );
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_state_is_not_silently_replaced_with_a_new_identity() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::temp_dir().join(format!(
            "drawdock-trust-test-{}",
            random_hex::<8>().unwrap()
        ));
        let store = TrustStore::open(directory.clone()).unwrap();
        let path = store.path.clone();
        drop(store);
        fs::write(&path, b"invalid protected state").unwrap();
        assert!(TrustStore::open(directory.clone()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"invalid protected state");
        fs::remove_dir_all(directory).unwrap();
    }
}
