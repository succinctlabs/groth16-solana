use clap::Parser;
use groth16_solana::{verify_proof, SP1ProofFixture, GROTH16_VK_BYTES};
use num_bigint::BigUint;
use num_traits::Num;
use sp1_sdk::{utils, ProverClient, SP1ProofWithPublicValues, SP1Stdin};
use std::str::FromStr;
use strum_macros::{Display, EnumIter, EnumString};

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
pub const FIBONACCI_ELF: &[u8] = include_bytes!("../../elfs/fibonacci-riscv32im-succinct-zkvm-elf");
pub const ISPRIME_ELF: &[u8] = include_bytes!("../../elfs/isprime-riscv32im-succinct-zkvm-elf");
pub const SHA2_ELF: &[u8] = include_bytes!("../../elfs/sha2-riscv32im-succinct-zkvm-elf");
pub const TENDERMINT_ELF: &[u8] =
    include_bytes!("../../elfs/tendermint-riscv32im-succinct-zkvm-elf");

pub fn proof_to_fixture(sp1_proof_with_public_values: SP1ProofWithPublicValues) -> SP1ProofFixture {
    let proof = sp1_proof_with_public_values
        .proof
        .try_as_groth_16()
        .expect("Failed to convert proof to Groth16 proof");

    let raw_proof = hex::decode(proof.raw_proof).unwrap();

    // Convert public inputs to byte representations.
    let vkey_hash = BigUint::from_str_radix(&proof.public_inputs[0], 10)
        .unwrap()
        .to_bytes_be();
    let committed_values_digest = BigUint::from_str_radix(&proof.public_inputs[1], 10)
        .unwrap()
        .to_bytes_be();

    let public_inputs = [vkey_hash.to_vec(), committed_values_digest.to_vec()].concat();

    SP1ProofFixture {
        proof: raw_proof,
        public_inputs,
    }
}

#[derive(clap::Parser)]
#[command(name = "zkVM Proof Generator")]
struct Cli {
    #[arg(
        long,
        value_name = "ELF",
        default_value = "fibonacci",
        help = "Specifies the ELF file to use (e.g., fibonacci, is-prime)"
    )]
    elf: String,
    #[arg(
        long,
        value_name = "prove",
        default_value = "false",
        help = "Specifies the ELF file to use (e.g., fibonacci, is-prime)"
    )]
    prove: bool,
}

#[derive(Debug, EnumString, EnumIter, Display)]
enum Elf {
    #[strum(serialize = "fibonacci")]
    Fibonacci,
    #[strum(serialize = "is-prime")]
    IsPrime,
    #[strum(serialize = "sha2")]
    Sha2,
    #[strum(serialize = "tendermint")]
    Tendermint,
}

impl Elf {
    fn get_elf(&self) -> &'static [u8] {
        match self {
            Elf::Fibonacci => FIBONACCI_ELF,
            Elf::IsPrime => ISPRIME_ELF,
            Elf::Sha2 => SHA2_ELF,
            Elf::Tendermint => TENDERMINT_ELF,
        }
    }
}

fn main() {
    // Setup logging for the application.
    utils::setup_logger();

    // Parse command line arguments.
    let args = Cli::parse();
    let mut stdin = SP1Stdin::new();

    let elf_enum = Elf::from_str(&args.elf)
        .expect("Invalid ELF name. Use 'fibonacci', 'is-prime', or other valid ELF names.");
    let elf = match elf_enum {
        Elf::Fibonacci => {
            let n = 20;
            stdin.write(&n);
            elf_enum.get_elf()
        }
        Elf::IsPrime => {
            let n = 11u64;
            stdin.write(&n);
            elf_enum.get_elf()
        }
        Elf::Sha2 | Elf::Tendermint => elf_enum.get_elf(),
    };

    // Where to save / load the proof from.
    let proof_file = format!("../binaries/{}_proof.bin", args.elf);

    // Only generate a proof if the prove flag is set.
    if args.prove {
        // Initialize the prover client
        let client = ProverClient::new();
        let (pk, _) = client.setup(elf);

        // Generate a proof for the specified program.
        let proof = client
            .prove(&pk, stdin)
            .groth16()
            .run()
            .expect("Groth16 proof generation failed");

        // Save the generated proof to `proof_file`.
        proof.save(&proof_file).unwrap();
    }

    // Load the proof from the file, and convert it to a fixture.
    let sp1_proof_with_public_values = SP1ProofWithPublicValues::load(&proof_file).unwrap();
    let fixture = proof_to_fixture(sp1_proof_with_public_values);
    let fixture_file = format!("../binaries/{}_fixture.bin", args.elf);

    // Serialize the fixture using borsh and write it to the fixture file
    let serialized_fixture = borsh::to_vec(&fixture).expect("Failed to serialize fixture");
    std::fs::write(&fixture_file, serialized_fixture).expect("Failed to write fixture to file");
    println!("Fixture saved to {}", fixture_file);

    verify_proof(&fixture.proof, &fixture.public_inputs, GROTH16_VK_BYTES)
        .expect("Proof verification failed");

    println!("Successfully verified proof for the program!")
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;
    use num_traits::Num;
    use strum::IntoEnumIterator;

    #[test]
    fn test_programs() {
        Elf::iter().for_each(|program| {
            let fixture_file = format!("../binaries/{}_fixture.bin", program.to_string());
            // Read the serialized fixture from the file.
            let serialized_fixture =
                std::fs::read(&fixture_file).expect("Failed to read fixture file");

            // Deserialize the fixture using borsh.
            let fixture: SP1ProofFixture =
                borsh::from_slice(&serialized_fixture).expect("Failed to deserialize fixture");

            // Verify the proof.
            let result = verify_proof(&fixture.proof, &fixture.public_inputs, GROTH16_VK_BYTES);

            assert!(result.is_ok(), "Proof verification failed for {}", program);
        });
    }
}
