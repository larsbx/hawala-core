use super::*;
use hawala_iroh_bridge::{handshake, IDENTITY_ALPN};
use iroh::{endpoint::presets, Endpoint, EndpointAddr};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_report_ack_reconnect_and_replay_preserve_the_original_claim_after_restart() {
    for (holder_machine, cash_machine) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let fixture = fixture(holder_machine, cash_machine, false);
        let (lot, request, custody) = custody_fixture(&fixture);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transport-recovery.sqlite");
        let sim = Simulator::open(&path).unwrap();
        seeded(&sim, 3, 20, 1000);
        sim.register_test_lot(&lot).unwrap();
        sim.admit(
            &fixture.envelope(),
            &fixture.policy,
            &custody,
            &request,
            140,
        )
        .unwrap();
        let holder = if holder_machine { 7 } else { 2 };
        let cash = if cash_machine { 8 } else { 4 };
        assert!(matches!(
            sim.claim_test_dispatch([10; 32], [22; 32], id(cash), 3, 150)
                .unwrap(),
            TestDispatchDecision::Claimed(_)
        ));
        let baseline = super::conformance::projection(&sim, &path, 1, 0);
        drop(sim);
        let signed = report(cash, ObservationKind::Timeout);
        let wire = signed.encode();
        let mut previous_endpoint = None;

        // Pass 1 persists the report and drops the connection before the application ACK.
        // Pass 2 reopens the journal and resends the identical report over fresh TLS keys.
        for replay in [false, true] {
            let sim = Simulator::open(&path).unwrap();
            let sender = Endpoint::builder(presets::Minimal)
                .alpns(vec![IDENTITY_ALPN.to_vec()])
                .bind_addr("127.0.0.1:0")
                .unwrap()
                .bind()
                .await
                .unwrap();
            let receiver = Endpoint::builder(presets::Minimal)
                .alpns(vec![IDENTITY_ALPN.to_vec()])
                .bind_addr("127.0.0.1:0")
                .unwrap()
                .bind()
                .await
                .unwrap();
            assert_ne!(previous_endpoint, Some(receiver.id()));
            previous_endpoint = Some(receiver.id());
            let target = EndpointAddr::new(receiver.id()).with_ip_addr(receiver.bound_sockets()[0]);
            let sender_nonces = RecoveryHandshakeNonces::default();
            let receiver_nonces = RecoveryHandshakeNonces::default();
            tokio::time::timeout(Duration::from_secs(30), async {
                tokio::join!(
                    async {
                        let connection = sender.connect(target, IDENTITY_ALPN).await.unwrap();
                        handshake::initiate(
                            &sender,
                            &connection,
                            &key(cash),
                            id(holder),
                            &sender_nonces,
                        )
                        .await
                        .unwrap();
                        let mut tx = connection.open_uni().await.unwrap();
                        tx.write_all(&wire).await.unwrap();
                        tx.finish().unwrap();
                        // The receiver may close after commit before the stream ACK reaches us.
                        let _ = tx.stopped().await;
                        let ack = connection.accept_uni().await;
                        if replay {
                            let mut ack = ack.expect("replayed report gets an application ACK");
                            assert_eq!(ack.read_to_end(1).await.unwrap(), [1]);
                        } else {
                            assert!(ack.is_err(), "first application ACK was deliberately lost");
                        }
                    },
                    async {
                        let connection = receiver.accept().await.unwrap().await.unwrap();
                        let peer = handshake::accept(
                            &receiver,
                            &connection,
                            &key(holder),
                            id(cash),
                            &receiver_nonces,
                        )
                        .await
                        .unwrap();
                        let mut rx = connection.accept_uni().await.unwrap();
                        let bytes = rx.read_to_end(OBSERVATION_FRAME_LEN).await.unwrap();
                        assert_eq!(bytes, wire, "recovery preserves the exact signed frame");
                        let observation = SignedObservation::decode(&bytes).unwrap();
                        assert_eq!(
                            sim.record_observation(
                                &observation,
                                peer.identity(),
                                if replay { 1000 } else { 150 }
                            )
                            .unwrap(),
                            if replay {
                                ObservationOutcome::IdenticalReplayHeld
                            } else {
                                ObservationOutcome::RecordedHeld
                            }
                        );
                        if replay {
                            let mut ack = connection.open_uni().await.unwrap();
                            ack.write_all(&[1]).await.unwrap();
                            ack.finish().unwrap();
                            ack.stopped().await.unwrap();
                        } else {
                            connection
                                .close(0u32.into(), b"injected disconnect after durable report");
                        }
                    }
                );
            })
            .await
            .expect("report recovery exchange exceeded its deadline");
            sender.close().await;
            receiver.close().await;
            assert_eq!(super::conformance::projection(&sim, &path, 1, 0), baseline);
            assert_eq!(
                sim.claim_test_dispatch([10; 32], [22; 32], id(cash), 3, 150)
                    .unwrap(),
                TestDispatchDecision::Held
            );
            assert_eq!(
                sim.admit(
                    &fixture.envelope(),
                    &fixture.policy,
                    &custody,
                    &request,
                    150
                )
                .unwrap(),
                Admission::Held
            );
            assert_eq!(
                sim.test_outbox_inventory().unwrap(),
                vec![([10; 32], TestOutboxState::ClaimedUncertain)]
            );
            let sql = rusqlite::Connection::open(&path).unwrap();
            let count: u64 = sql
                .query_row("SELECT count(*) FROM recovery_observations", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1);
        }
    }
}
