use std::time::Instant;
use vm_provider::progress::DockerProgressParser;

fn main() {
    let iterations = 10_000;
    println!("Benchmarking DockerProgressParser instantiation ({} iterations)...", iterations);

    let start = Instant::now();
    for _ in 0..iterations {
        let _parser = DockerProgressParser::new();
    }
    let duration = start.elapsed();

    println!("Total time: {:?}", duration);
    println!("Average time per instantiation: {:?}", duration / iterations);
}
