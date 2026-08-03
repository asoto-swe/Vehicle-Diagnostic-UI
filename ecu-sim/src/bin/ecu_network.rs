fn main() {
    let iface = "vcan0";
    println!("simulated vehicle network on {iface}");
    println!("  bms      req=0x7E0 resp=0x7E8");
    println!("  motor    req=0x7E1 resp=0x7E9");
    println!("  thermal  req=0x7E2 resp=0x7EA  (broadcasts coolant status on 0x300)");

    let _bms = ecu_sim::fault_engine::spawn_bms(iface);
    let _motor = ecu_sim::fault_engine::spawn_motor(iface);
    let _thermal = ecu_sim::fault_engine::spawn_thermal(iface);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
