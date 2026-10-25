use super::*;
use rusqlite::OptionalExtension;
use std::collections::BTreeMap;
#[test]
fn lean_observation_traces_preserve_durable_financial_state() {
    let cases:BTreeMap<(u8,&str),[i64;3]>=include_str!("../../../../formal/traces/observations.tsv")
        .lines().filter(|l|!l.starts_with('#')).map(|line|{
            let p:Vec<_>=line.split('\t').collect();assert_eq!(p.len(),5);
            ((p[0].parse().unwrap(),p[1]),[p[2].parse().unwrap(),p[3].parse().unwrap(),p[4].parse().unwrap()])
        }).collect();
    assert_eq!(cases.len(),105);
    for (holder_machine,cash_machine) in [(false,false),(false,true),(true,false),(true,true)] {
        for ((context,trace),_) in &cases {
            let f=fixture(holder_machine,cash_machine,false);let (lot,r,c)=custody_fixture(&f);
            let dir=tempfile::tempdir().unwrap();let path=dir.path().join("observation-conformance.sqlite");
            let mut sim=Some(Simulator::open(&path).unwrap());let db=sim.as_ref().unwrap();
            seeded(db,3,20,1000);db.register_test_lot(&lot).unwrap();
            let peer=f.qr.request().verifier;let cash=if cash_machine {8}else{4};
            if *context!=0 {db.admit(&f.envelope(),&f.policy,&c,&r,150).unwrap();}
            let mut claims=0;
            if *context==2 {
                assert!(matches!(db.claim_test_dispatch([10;32],[22;32],peer,3,150).unwrap(),TestDispatchDecision::Claimed(_)));claims=1;
            }
            if *context==3 {assert_eq!(db.claim_test_dispatch([10;32],[22;32],peer,3,175).unwrap(),TestDispatchDecision::Held);}
            if *context==4 {rusqlite::Connection::open(&path).unwrap().execute("UPDATE sim_state SET epoch=4 WHERE id=1",[]).unwrap();}
            let baseline=super::conformance::projection(db,&path,claims,0);
            let mut prefix=String::new();
            for event in trace.split(',') {
                if event=="restart" {drop(sim.take());sim=Some(Simulator::open(&path).unwrap());}
                else {
                    let kind=if event.ends_with("Delivery") || event=="delivery" {ObservationKind::ReportedDelivery}else{ObservationKind::Timeout};
                    let signed=report(cash,kind);
                    let actor=if event.starts_with("invalid") {id(9)}else{peer};
                    sim.as_ref().unwrap().record_observation(&signed,actor,150).unwrap();
                }
                if !prefix.is_empty(){prefix.push(',');}prefix.push_str(event);
                let db=sim.as_ref().unwrap();let sql=rusqlite::Connection::open(&path).unwrap();
                let frame:Option<Vec<u8>>=sql.query_row("SELECT frame FROM recovery_observations WHERE observation=?1",rusqlite::params![[80u8;32].as_slice()],|r|r.get(0)).optional().unwrap();
                let primary=frame.map(|b|match SignedObservation::decode(&b).unwrap().observation().kind {
                    ObservationKind::Timeout=>0,ObservationKind::ReportedDelivery=>1,_=>panic!("unmodeled kind")
                }).unwrap_or(-1);
                let conflicts:i64=sql.query_row("SELECT count(*)>0 FROM recovery_conflicts",[],|r|r.get(0)).unwrap();
                let uncertain:i64=sql.query_row("SELECT count(*)>0 FROM admission_receipts WHERE state='uncertain'",[],|r|r.get(0)).unwrap();
                assert_eq!([primary,conflicts,uncertain],*cases.get(&(*context,prefix.as_str())).expect("exported prefix"),"observation trace {context}/{prefix}");
                let mut financial=super::conformance::projection(db,&path,claims,0);
                financial[5]=baseline[5]; // uncertainty is the only permitted lifecycle mutation
                assert_eq!(financial,baseline,"report changed financial projection");
            }
        }
    }
}
