use std::net::TcpListener;

use super::*;

fn bind_ephemeral() -> TcpListener {
    TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port")
}

fn port_of(listener: &TcpListener) -> u16 {
    listener.local_addr().expect("read local addr").port()
}

#[test]
fn allocation_prefers_requested_port() {
    let mut last_error = None;
    for _ in 0..20 {
        let probe = bind_ephemeral();
        let port = port_of(&probe);
        drop(probe);
        match allocate_in_range(PortService::Mcp, Some(port), port, port) {
            Ok((listener, allocation)) => {
                assert_eq!(allocation.port, port);
                assert_eq!(allocation.switched_from, None);
                drop(listener);
                return;
            }
            Err(error) => last_error = Some(error),
        }
    }
    panic!("无法在空闲端口上完成分配: {last_error:?}");
}

#[test]
fn allocation_skips_occupied_preferred_port() {
    let occupied = bind_ephemeral();
    let occupied_port = port_of(&occupied);
    let (start, end) = (occupied_port, occupied_port.saturating_add(100));

    let (listener, allocation) =
        allocate_in_range(PortService::Aria2, Some(occupied_port), start, end)
            .expect("段内应当仍有可用端口");

    assert_ne!(allocation.port, occupied_port);
    assert_eq!(allocation.switched_from, Some(occupied_port));
    drop(listener);
}

#[test]
fn allocation_reports_exhaustion() {
    let occupied = bind_ephemeral();
    let port = port_of(&occupied);

    let error = allocate_in_range(PortService::JxxkDownload, None, port, port)
        .expect_err("唯一候选端口被占用时应报告耗尽");

    match error {
        PortError::Exhausted {
            service,
            start,
            end,
        } => {
            assert_eq!(service, PortService::JxxkDownload);
            assert_eq!((start, end), (port, port));
        }
    }
}

#[test]
fn preferred_outside_range_is_ignored() {
    let occupied = bind_ephemeral();
    let start = port_of(&occupied);
    let end = start.saturating_add(100);

    let (listener, allocation) = allocate_in_range(PortService::Aria2, Some(1), start, end)
        .expect("段内应当有可用端口");

    assert_ne!(allocation.port, start);
    assert_eq!(allocation.switched_from, None);
    drop(listener);
}

#[test]
fn distinct_allocation_returns_the_requested_amount() {
    let free = bind_ephemeral();
    let start = port_of(&free);
    drop(free);
    let end = start.saturating_add(50);

    let allocations =
        allocate_distinct_in_range(PortService::Mesh, start, end, 4).expect("段内应能分配 4 个端口");

    assert_eq!(allocations.len(), 4);
    let mut ports: Vec<u16> = allocations.iter().map(|allocation| allocation.port).collect();
    ports.sort_unstable();
    assert!(ports.windows(2).all(|pair| pair[0] != pair[1]));
    assert!(ports.iter().all(|port| (start..=end).contains(port)));
    assert!(allocations.iter().all(|allocation| allocation.switched_from.is_none()));
}

#[test]
fn distinct_allocation_skips_occupied_ports() {
    let occupied = bind_ephemeral();
    let occupied_port = port_of(&occupied);
    let end = occupied_port.saturating_add(50);

    let allocations = allocate_distinct_in_range(PortService::Mesh, occupied_port, end, 2)
        .expect("占用端口之外应能分配 2 个端口");

    assert!(allocations
        .iter()
        .all(|allocation| allocation.port != occupied_port));
    assert!(allocations
        .iter()
        .all(|allocation| (occupied_port..=end).contains(&allocation.port)));
}

#[test]
fn distinct_allocation_reports_exhaustion_when_range_is_too_small() {
    let occupied = bind_ephemeral();
    let port = port_of(&occupied);

    let error =
        allocate_distinct_in_range(PortService::JxxkDownload, port, port, 2).expect_err("候选数不足应报告耗尽");

    match error {
        PortError::Exhausted { service, start, end } => {
            assert_eq!(service, PortService::JxxkDownload);
            assert_eq!((start, end), (port, port));
        }
    }
}
