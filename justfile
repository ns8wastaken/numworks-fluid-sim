app_name := "FluidSim"
lib_filename := "libfluid_sim"

icon_file := "assets/icon.png"

host_triple := shell("rustc -vV | grep \"host:\" | awk '{print $2}'")
thumb_target := "thumbv7em-none-eabihf"

build-epsilon:
    cargo build --release --bin {{app_name}} --target={{thumb_target}} --features "epsilon" --no-default-features

send-epsilon: build-epsilon
    npm exec --yes -- nwlink@0.0.19 install-nwa ./target/{{thumb_target}}/release/{{app_name}}

check:
    cargo check --release --bin {{app_name}} --target={{thumb_target}} --features "epsilon" --no-default-features
    cargo check --release --target={{host_triple}} --lib --features "epsilon" --no-default-features
    @echo All checks passed!

[linux]
run_nwb:
    ./simulator/output/release/simulator/linux/epsilon.bin --nwb ./target/{{host_triple}}/release/{{lib_filename}}.so

sim jobs="1":
    if [ ! -f "./simulator/output/release/simulator/linux/epsilon.bin" ]; then \
        cd simulator && . ../.venv/bin/activate && make PLATFORM=simulator -j{{jobs}}; \
    fi
    cargo build --release --target={{host_triple}} --lib --features "epsilon" --no-default-features
    just run_nwb

[confirm("This will clean the built app. Do you want to continue ?")]
clean:
    rm -f ./app.elf ./app.icon
    cargo clean

[confirm("This will clean the built app AND DELETE the simulator. Do you want to continue ?")]
clear:
    rm -rf ./simulator
    rm -f ./app.elf ./app.icon ./icon.png
    cargo clean
