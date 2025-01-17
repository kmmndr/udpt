use crate::server::Events;
use log::{error, trace};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;

const TWO_HOURS: std::time::Duration = std::time::Duration::from_secs(3600 * 2);

#[derive(Deserialize, Clone, PartialEq)]
pub enum TrackerMode {
    /// In static mode torrents are tracked only if they were added ahead of time.
    #[serde(rename = "static")]
    Static,

    /// In dynamic mode, torrents are tracked being added ahead of time.
    #[serde(rename = "dynamic")]
    Dynamic,

    /// Tracker will only serve authenticated peers.
    #[serde(rename = "private")]
    Private,
}

#[derive(Clone, Serialize)]
pub struct TorrentPeer {
    ip: std::net::SocketAddr,
    uploaded: u64,
    downloaded: u64,
    left: u64,
    event: Events,
    #[serde(serialize_with = "ser_instant")]
    updated: std::time::Instant,
}

fn ser_instant<S: serde::Serializer>(inst: &std::time::Instant, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_u64(inst.elapsed().as_millis() as u64)
}

#[derive(Ord, PartialOrd, PartialEq, Eq, Clone)]
pub struct InfoHash {
    info_hash: [u8; 20],
}

impl std::fmt::Display for InfoHash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut chars = [0u8; 40];
        binascii::bin2hex(&self.info_hash, &mut chars).expect("failed to hexlify");
        write!(f, "{}", std::str::from_utf8(&chars).unwrap())
    }
}

impl std::str::FromStr for InfoHash {
    type Err = binascii::ConvertError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut i = Self { info_hash: [0u8; 20] };
        if s.len() != 40 {
            return Err(binascii::ConvertError::InvalidInputLength);
        }
        binascii::hex2bin(s.as_bytes(), &mut i.info_hash)?;
        Ok(i)
    }
}

impl std::convert::From<&[u8]> for InfoHash {
    fn from(data: &[u8]) -> InfoHash {
        assert_eq!(data.len(), 20);
        let mut ret = InfoHash { info_hash: [0u8; 20] };
        ret.info_hash.clone_from_slice(data);
        ret
    }
}

impl From<[u8; 20]> for InfoHash {
    fn from(info_hash: [u8; 20]) -> Self {
        InfoHash {
            info_hash,
        }
    }
}

impl serde::ser::Serialize for InfoHash {
    fn serialize<S: serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buffer = [0u8; 40];
        let bytes_out = binascii::bin2hex(&self.info_hash, &mut buffer).ok().unwrap();
        let str_out = std::str::from_utf8(bytes_out).unwrap();

        serializer.serialize_str(str_out)
    }
}

struct InfoHashVisitor;

impl<'v> serde::de::Visitor<'v> for InfoHashVisitor {
    type Value = InfoHash;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a 40 character long hash")
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        if v.len() != 40 {
            return Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(v),
                &"expected a 40 character long string",
            ));
        }

        let mut res = InfoHash { info_hash: [0u8; 20] };

        if binascii::hex2bin(v.as_bytes(), &mut res.info_hash).is_err() {
            return Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(v),
                &"expected a hexadecimal string",
            ));
        } else {
            Ok(res)
        }
    }
}

impl<'de> serde::de::Deserialize<'de> for InfoHash {
    fn deserialize<D: serde::de::Deserializer<'de>>(des: D) -> Result<Self, D::Error> {
        des.deserialize_str(InfoHashVisitor)
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq)]
pub struct PeerId([u8; 20]);
impl PeerId {
    pub fn from_array(v: &[u8; 20]) -> &PeerId {
        unsafe {
            // This is safe since PeerId's repr is transparent and content's are identical. PeerId == [0u8; 20]
            core::mem::transmute(v)
        }
    }

    pub fn get_client_name(&self) -> Option<&'static str> {
        if self.0[0] == b'M' {
            return Some("BitTorrent");
        }
        if self.0[0] == b'-' {
            let name = match &self.0[1..3] {
                b"AG" => "Ares",
                b"A~" => "Ares",
                b"AR" => "Arctic",
                b"AV" => "Avicora",
                b"AX" => "BitPump",
                b"AZ" => "Azureus",
                b"BB" => "BitBuddy",
                b"BC" => "BitComet",
                b"BF" => "Bitflu",
                b"BG" => "BTG (uses Rasterbar libtorrent)",
                b"BR" => "BitRocket",
                b"BS" => "BTSlave",
                b"BX" => "~Bittorrent X",
                b"CD" => "Enhanced CTorrent",
                b"CT" => "CTorrent",
                b"DE" => "DelugeTorrent",
                b"DP" => "Propagate Data Client",
                b"EB" => "EBit",
                b"ES" => "electric sheep",
                b"FT" => "FoxTorrent",
                b"FW" => "FrostWire",
                b"FX" => "Freebox BitTorrent",
                b"GS" => "GSTorrent",
                b"HL" => "Halite",
                b"HN" => "Hydranode",
                b"KG" => "KGet",
                b"KT" => "KTorrent",
                b"LH" => "LH-ABC",
                b"LP" => "Lphant",
                b"LT" => "libtorrent",
                b"lt" => "libTorrent",
                b"LW" => "LimeWire",
                b"MO" => "MonoTorrent",
                b"MP" => "MooPolice",
                b"MR" => "Miro",
                b"MT" => "MoonlightTorrent",
                b"NX" => "Net Transport",
                b"PD" => "Pando",
                b"qB" => "qBittorrent",
                b"QD" => "QQDownload",
                b"QT" => "Qt 4 Torrent example",
                b"RT" => "Retriever",
                b"S~" => "Shareaza alpha/beta",
                b"SB" => "~Swiftbit",
                b"SS" => "SwarmScope",
                b"ST" => "SymTorrent",
                b"st" => "sharktorrent",
                b"SZ" => "Shareaza",
                b"TN" => "TorrentDotNET",
                b"TR" => "Transmission",
                b"TS" => "Torrentstorm",
                b"TT" => "TuoTu",
                b"UL" => "uLeecher!",
                b"UT" => "µTorrent",
                b"UW" => "µTorrent Web",
                b"VG" => "Vagaa",
                b"WD" => "WebTorrent Desktop",
                b"WT" => "BitLet",
                b"WW" => "WebTorrent",
                b"WY" => "FireTorrent",
                b"XL" => "Xunlei",
                b"XT" => "XanTorrent",
                b"XX" => "Xtorrent",
                b"ZT" => "ZipTorrent",
                _ => return None,
            };
            Some(name)
        } else {
            None
        }
    }
}
impl Serialize for PeerId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer, {
        let mut tmp = [0u8; 40];
        binascii::bin2hex(&self.0, &mut tmp).unwrap();
        let id = std::str::from_utf8(&tmp).ok();

        #[derive(Serialize)]
        struct PeerIdInfo<'a> {
            id: Option<&'a str>,
            client: Option<&'a str>,
        }

        let obj = PeerIdInfo {
            id,
            client: self.get_client_name(),
        };
        obj.serialize(serializer)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TorrentEntry {
    is_flagged: bool,

    #[serde(skip)]
    peers: std::collections::BTreeMap<PeerId, TorrentPeer>,

    completed: u32,

    #[serde(skip)]
    seeders: u32,
}

impl TorrentEntry {
    pub fn new() -> TorrentEntry {
        TorrentEntry {
            is_flagged: false,
            peers: std::collections::BTreeMap::new(),
            completed: 0,
            seeders: 0,
        }
    }

    pub fn is_flagged(&self) -> bool {
        self.is_flagged
    }

    pub fn update_peer(
        &mut self, peer_id: &PeerId, remote_address: &std::net::SocketAddr, uploaded: u64, downloaded: u64, left: u64,
        event: Events,
    ) {
        let is_seeder = left == 0 && uploaded > 0;
        let mut was_seeder = false;
        let mut is_completed = left == 0 && (event as u32) == (Events::Complete as u32);
        if let Some(prev) = self.peers.insert(*peer_id, TorrentPeer {
            updated: std::time::Instant::now(),
            left,
            downloaded,
            uploaded,
            ip: *remote_address,
            event,
        }) {
            was_seeder = prev.left == 0 && prev.uploaded > 0;

            if is_completed && (prev.event as u32) == (Events::Complete as u32) {
                // don't update count again. a torrent should only be updated once per peer.
                is_completed = false;
            }
        }

        if is_seeder && !was_seeder {
            self.seeders += 1;
        } else if was_seeder && !is_seeder {
            self.seeders -= 1;
        }

        if is_completed {
            self.completed += 1;
        }
    }

    pub fn get_peers(&self, remote_addr: &std::net::SocketAddr) -> Vec<std::net::SocketAddr> {
        let mut list = Vec::new();
        for (_, peer) in self
            .peers
            .iter()
            .filter(|e| e.1.ip.is_ipv4() == remote_addr.is_ipv4())
            .take(74)
        {
            if peer.ip == *remote_addr {
                continue;
            }

            list.push(peer.ip);
        }
        list
    }

    pub fn get_peers_iter(&self) -> impl Iterator<Item = (&PeerId, &TorrentPeer)> {
        self.peers.iter()
    }

    pub fn get_stats(&self) -> (u32, u32, u32) {
        let leechers = (self.peers.len() as u32) - self.seeders;
        (self.seeders, self.completed, leechers)
    }
}

struct TorrentDatabase {
    torrent_peers: tokio::sync::RwLock<std::collections::BTreeMap<InfoHash, TorrentEntry>>,
}

impl Default for TorrentDatabase {
    fn default() -> Self {
        TorrentDatabase {
            torrent_peers: tokio::sync::RwLock::new(std::collections::BTreeMap::new()),
        }
    }
}

pub struct TorrentTracker {
    mode: TrackerMode,
    database: TorrentDatabase,
}

#[derive(Serialize, Deserialize)]
struct DatabaseRow<'a> {
    info_hash: InfoHash,
    entry: Cow<'a, TorrentEntry>,
}

pub enum TorrentStats {
    TorrentFlagged,
    TorrentNotRegistered,
    Stats { seeders: u32, leechers: u32, complete: u32 },
}

impl TorrentTracker {
    pub fn new(mode: TrackerMode) -> TorrentTracker {
        TorrentTracker {
            mode,
            database: TorrentDatabase {
                torrent_peers: RwLock::new(std::collections::BTreeMap::new()),
            },
        }
    }

    pub async fn load_database<R: tokio::io::AsyncRead + Unpin>(
        mode: TrackerMode, reader: &mut R,
    ) -> Result<TorrentTracker, std::io::Error> {
        let reader = tokio::io::BufReader::new(reader);
        let reader = async_compression::tokio::bufread::BzDecoder::new(reader);
        let reader = tokio::io::BufReader::new(reader);

        let res = TorrentTracker::new(mode);
        let mut db = res.database.torrent_peers.write().await;

        let mut records = reader.lines();
        loop {
            let line = match records.next_line().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(ref err) => {
                    error!("failed to read lines! {}", err);
                    continue;
                },
            };
            let row: DatabaseRow = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(err) => {
                    error!("failed to parse json: {}", err);
                    continue;
                }
            };
            let entry = row.entry.into_owned();
            let infohash = row.info_hash;
            db.insert(infohash, entry);
        }

        trace!("loaded {} entries from database", db.len());

        drop(db);

        Ok(res)
    }

    /// Adding torrents is not relevant to dynamic trackers.
    pub async fn add_torrent(&self, info_hash: &InfoHash) -> Result<(), ()> {
        let mut write_lock = self.database.torrent_peers.write().await;
        match write_lock.entry(info_hash.clone()) {
            std::collections::btree_map::Entry::Vacant(ve) => {
                ve.insert(TorrentEntry::new());
                Ok(())
            },
            std::collections::btree_map::Entry::Occupied(_entry) => Err(()),
        }
    }

    /// If the torrent is flagged, it will not be removed unless force is set to true.
    pub async fn remove_torrent(&self, info_hash: &InfoHash, force: bool) -> Result<(), ()> {
        use std::collections::btree_map::Entry;
        let mut entry_lock = self.database.torrent_peers.write().await;
        let torrent_entry = entry_lock.entry(info_hash.clone());
        match torrent_entry {
            Entry::Vacant(_) => {
                // no entry, nothing to do...
            }
            Entry::Occupied(entry) => {
                if force || !entry.get().is_flagged() {
                    entry.remove();
                    return Ok(());
                }
            }
        }
        Err(())
    }

    /// flagged torrents will result in a tracking error. This is to allow enforcement against piracy.
    pub async fn set_torrent_flag(&self, info_hash: &InfoHash, is_flagged: bool) -> bool {
        if let Some(entry) = self.database.torrent_peers.write().await.get_mut(info_hash) {
            if is_flagged && !entry.is_flagged {
                // empty peer list.
                entry.peers.clear();
            }
            entry.is_flagged = is_flagged;
            true
        } else {
            false
        }
    }

    pub async fn get_torrent_peers(
        &self, info_hash: &InfoHash, remote_addr: &std::net::SocketAddr,
    ) -> Option<Vec<std::net::SocketAddr>> {
        let read_lock = self.database.torrent_peers.read().await;
        read_lock
            .get(info_hash)
            .map(|entry| entry.get_peers(remote_addr))
    }

    pub async fn update_torrent_and_get_stats(
        &self, info_hash: &InfoHash, peer_id: &PeerId, remote_address: &std::net::SocketAddr, uploaded: u64,
        downloaded: u64, left: u64, event: Events,
    ) -> TorrentStats {
        use std::collections::btree_map::Entry;
        let mut torrent_peers = self.database.torrent_peers.write().await;
        let torrent_entry = match torrent_peers.entry(info_hash.clone()) {
            Entry::Vacant(vacant) => {
                match self.mode {
                    TrackerMode::Dynamic => vacant.insert(TorrentEntry::new()),
                    _ => {
                        return TorrentStats::TorrentNotRegistered;
                    }
                }
            }
            Entry::Occupied(entry) => {
                if entry.get().is_flagged() {
                    return TorrentStats::TorrentFlagged;
                }
                entry.into_mut()
            }
        };

        torrent_entry.update_peer(peer_id, remote_address, uploaded, downloaded, left, event);

        let (seeders, complete, leechers) = torrent_entry.get_stats();

        TorrentStats::Stats {
            seeders,
            leechers,
            complete,
        }
    }

    pub(crate) async fn get_database(&self) -> tokio::sync::RwLockReadGuard<'_, BTreeMap<InfoHash, TorrentEntry>> {
        self.database.torrent_peers.read().await
    }

    pub async fn save_database<W: tokio::io::AsyncWrite + Unpin>(&self, w: W) -> Result<(), std::io::Error> {
        use tokio::io::AsyncWriteExt;

        let mut writer = async_compression::tokio::write::BzEncoder::new(w);

        let db_lock = self.database.torrent_peers.read().await;

        let db: &BTreeMap<InfoHash, TorrentEntry> = &*db_lock;
        let mut tmp = Vec::with_capacity(4096);

        for row in db {
            let entry = DatabaseRow {
                info_hash: row.0.clone(),
                entry: Cow::Borrowed(row.1),
            };
            tmp.clear();
            if let Err(err) = serde_json::to_writer(&mut tmp, &entry) {
                error!("failed to serialize: {}", err);
                continue;
            };
            tmp.push(b'\n');
            writer.write_all(&tmp).await?;
        }
        writer.flush().await?;
        Ok(())
    }

    async fn cleanup(&self) {
        let mut lock = self.database.torrent_peers.write().await;
        let db: &mut BTreeMap<InfoHash, TorrentEntry> = &mut *lock;
        let mut torrents_to_remove = Vec::new();

        for (k, v) in db.iter_mut() {
            // timed-out peers..
            {
                let mut peers_to_remove = Vec::new();
                let torrent_peers = &mut v.peers;

                for (peer_id, state) in torrent_peers.iter() {
                    if state.updated.elapsed() > TWO_HOURS {
                        // over 2 hours past since last update...
                        peers_to_remove.push(*peer_id);
                    }
                }

                for peer_id in peers_to_remove.iter() {
                    torrent_peers.remove(peer_id);
                }
            }

            if self.mode == TrackerMode::Dynamic {
                // peer-less torrents..
                if v.peers.is_empty() && !v.is_flagged() {
                    torrents_to_remove.push(k.clone());
                }
            }
        }

        for info_hash in torrents_to_remove {
            db.remove(&info_hash);
        }
    }

    pub async fn periodic_task(&self, db_path: &str) {
        // cleanup db
        self.cleanup().await;

        // save journal db.
        let mut journal_path = std::path::PathBuf::from(db_path);

        let mut filename = String::from(journal_path.file_name().unwrap().to_str().unwrap());
        filename.push_str("-journal");

        journal_path.set_file_name(filename.as_str());
        let jp_str = journal_path.as_path().to_str().unwrap();

        // scope to make sure backup file is dropped/closed.
        {
            let mut file = match tokio::fs::File::create(jp_str).await {
                Err(err) => {
                    error!("failed to open file '{}': {}", db_path, err);
                    return;
                }
                Ok(v) => v,
            };
            trace!("writing database to {}", jp_str);
            if let Err(err) = self.save_database(&mut file).await {
                error!("failed saving database. {}", err);
                return;
            }
        }

        // overwrite previous db
        trace!("renaming '{}' to '{}'", jp_str, db_path);
        if let Err(err) = tokio::fs::rename(jp_str, db_path).await {
            error!("failed to move db backup. {}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_sync<T: Sync>() {}
    fn is_send<T: Send>() {}

    #[test]
    fn tracker_send() {
        is_send::<TorrentTracker>();
    }

    #[test]
    fn tracker_sync() {
        is_sync::<TorrentTracker>();
    }

    #[tokio::test]
    async fn test_save_db() {
        let tracker = TorrentTracker::new(TrackerMode::Dynamic);
        tracker
            .add_torrent(&[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0].into())
            .await
            .expect("failed to add torrent");

        let mut out = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut out);

        tracker.save_database(&mut cursor).await.expect("db save failed");
        assert!(cursor.position() > 0);
    }

    #[test]
    fn test_infohash_de() {
        use serde_json;

        let ih: InfoHash = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 1, 2, 3, 4, 5, 6, 7, 8, 9, 1].into();

        let serialized_ih = serde_json::to_string(&ih).unwrap();

        let de_ih: InfoHash = serde_json::from_str(serialized_ih.as_str()).unwrap();

        assert!(de_ih == ih);
    }
}
