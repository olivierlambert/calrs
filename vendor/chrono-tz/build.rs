fn main() {
    // calrs: chrono-tz-build only reruns on CHRONO_TZ_TIMEZONE_FILTER, so a
    // refreshed tz/ would otherwise be ignored by an existing target/.
    println!("cargo:rerun-if-changed=tz");
    chrono_tz_build::main();
}
