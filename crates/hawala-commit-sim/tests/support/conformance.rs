//! Finite trace conformance, not a proof of Rust semantics or SQLite durability.
use super::*;
use std::collections::BTreeMap;

fn projection(sim:&Simulator,path:&std::path::Path,claims:u64,fenced_claims:u64)->[u64;10] {
    let sql=rusqlite::Connection::open(path).unwrap();
    let committed=sim.event_count().unwrap();
    let nonce:u64=sql.query_row("SELECT count(*) FROM sim_events WHERE nonce=?1",rusqlite::params![[24u8;32].as_slice()],|r|r.get(0)).unwrap();
    let claimed:u64=sql.query_row("SELECT count(*) FROM test_outbox WHERE state='claimed'",[],|r|r.get(0)).unwrap();
    let uncertain:u64=sql.query_row("SELECT count(*) FROM admission_receipts WHERE state='uncertain'",[],|r|r.get(0)).unwrap();
    let expired:u64=sql.query_row("SELECT count(*) FROM test_outbox WHERE state='held'",[],|r|r.get(0)).unwrap();
    let current:u64=sql.query_row("SELECT epoch=3 FROM sim_state WHERE id=1",[],|r|r.get(0)).unwrap();
    assert_eq!(sim.balance([41;32]).unwrap(),Some(1000));
    assert_eq!(sim.state_version().unwrap(),20+committed);
    [sim.balance([42;32]).unwrap().unwrap(),sim.balance([43;32]).unwrap().unwrap(),
     committed,nonce,claimed,uncertain,claims,expired,current,fenced_claims]
}
#[test]
fn lean_generated_transition_cover_matches_durable_rust_for_all_peer_modes() {
    let cases:BTreeMap<&str,[u64;10]>=include_str!("../../../../formal/traces/lifecycle.tsv")
        .lines().filter(|l|!l.starts_with('#')).map(|line| {
            let mut parts=line.split('\t');let path=parts.next().unwrap();
            let fields:Vec<u64>=parts.map(|v|v.parse().unwrap()).collect();
            (path,fields.try_into().expect("exact ten-field projection"))
        }).collect();
    assert_eq!(cases.len(),100,"ten states times ten events");
    for (holder_machine,cash_machine) in [(false,false),(false,true),(true,false),(true,true)] {
        for (trace,_) in &cases {
            let mut f=fixture(holder_machine,cash_machine,false);let (lot,r,c)=custody_fixture(&f);
            let dir=tempfile::tempdir().unwrap();let path=dir.path().join("conformance.sqlite");
            let mut sim=Some(Simulator::open(&path).unwrap());
            seeded(sim.as_ref().unwrap(),3,20,1000);sim.as_ref().unwrap().register_test_lot(&lot).unwrap();
            let mut claims=0;let mut fenced_claims=0;let mut prefix=String::new();
            assert_eq!(projection(sim.as_ref().unwrap(),&path,claims,fenced_claims),[1000,0,0,0,0,0,0,0,1,0]);
            for event in trace.split(',') {
                match event {
                    "admit"=>{sim.as_ref().unwrap().admit(&f.envelope(),&f.policy,&c,&r,150).unwrap();},
                    "denied"=>{
                        let reviews=f.reviews;f.reviews.custody_asset_control=Review::Denied;
                        sim.as_ref().unwrap().admit(&f.envelope(),&f.policy,&c,&r,150).unwrap();f.reviews=reviews;
                    },
                    "claim"|"stale"|"reject"|"expire"=>{
                        let peer=if event=="reject" {id(9)}else{f.qr.request().verifier};
                        let epoch=if event=="stale" {4}else{3};
                        if let TestDispatchDecision::Claimed(_)=sim.as_ref().unwrap().claim_test_dispatch([10;32],[22;32],peer,epoch,if event=="expire"{175}else{150}).unwrap() {
                            claims+=1;
                            let current:bool=rusqlite::Connection::open(&path).unwrap().query_row("SELECT epoch=3 FROM sim_state WHERE id=1",[],|r|r.get(0)).unwrap();
                            if !current {fenced_claims+=1;}
                        }
                    },
                    "fence"=>{
                        // Test-only injection of an externally changed persisted epoch.
                        // This is not an operational authority handoff API.
                        rusqlite::Connection::open(&path).unwrap().execute("UPDATE sim_state SET epoch=4 WHERE id=1",[]).unwrap();
                    },
                    "hold"=>{
                        let db=sim.as_ref().unwrap();let result=db.mark_uncertain([10;32]);
                        if db.event_count().unwrap()==0 {assert!(result.is_err());}else{result.unwrap();}
                    },
                    "crash"=>{drop(sim.take());sim=Some(Simulator::open(&path).unwrap());},
                    "replay"=>{
                        let db=sim.as_ref().unwrap();
                        if db.event_count().unwrap()>0 {db.admit(&f.envelope(),&f.policy,&c,&r,150).unwrap();}
                        else {assert_eq!(db.event_count().unwrap(),0);}
                    },
                    _=>panic!("unknown exported event"),
                }
                if !prefix.is_empty(){prefix.push(',');}prefix.push_str(event);
                assert_eq!(projection(sim.as_ref().unwrap(),&path,claims,fenced_claims),*cases.get(prefix.as_str()).expect("prefix exported"),
                    "trace projection mismatch: {prefix}, modes {holder_machine}/{cash_machine}");
            }
        }
    }
}
