use anyhow::{Result, anyhow};
use bitcoin::{ScriptBuf, XOnlyPublicKey, opcodes, script::Builder, Sequence, PublicKey};
use bitcoin::taproot::TaprootBuilder;
use bitcoin::secp256k1::Secp256k1;
use ark_core::UNSPENDABLE_KEY;

pub struct ScriptManager;

impl ScriptManager {
    pub fn new() -> Self {
        Self
    }

    // CSV signature script (exit path with timelock)
    pub fn csv_sig_script(
        &self,
        locktime: Sequence,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> ScriptBuf {
        ScriptBuf::builder()
            .push_int(locktime.to_consensus_u32() as i64)
            .push_opcode(opcodes::all::OP_CSV) 
            .push_opcode(opcodes::all::OP_DROP)
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script()
    }

    // multisig script (collaborative path)
    pub fn multisig_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> ScriptBuf {
        ScriptBuf::builder()
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script()
    }

    // 3-party multisig script (user + counterparty + server)
    // pattern: Alice checksigverify Bob checksigverify Server checksig
    pub fn three_party_multisig_script(
        &self,
        pk_0: XOnlyPublicKey,
        pk_1: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> ScriptBuf {
        ScriptBuf::builder()
            .push_x_only_key(&pk_0)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&pk_1)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script()
    }

    // multi_a script (checksigadd variant from arkade-os tapscripts)
    // refer arkade-os/tapscripts vtxo example: multi_a(2, pk1, pk2)
    pub fn multi_a_script(
        &self,
        threshold: u8,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> ScriptBuf {
        ScriptBuf::builder()
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGADD)
            .push_int(threshold as i64)
            .push_opcode(opcodes::all::OP_NUMEQUAL)
            .into_script()
    }

    // simple exit script (user-only with CSV delay)
    // refer arkade-os/tapscripts vtxo example: and_v(v:pk(user), older(delay))
    pub fn create_exit_script(
        &self,
        user_pk: XOnlyPublicKey,
        delay: u32,
    ) -> Result<ScriptBuf> {
        let script = ScriptBuf::builder()
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_int(delay as i64)
            .push_opcode(opcodes::all::OP_CSV)  // Using OP_CSV
            .into_script();

        tracing::debug!("Created exit script for user: {} with delay: {}", user_pk, delay);
        Ok(script)
    }

    // checksigverify script (preferred for n-of-n)
    // pattern: A checksigverify B checksigverify (without final checksig)
    pub fn create_checksigverify_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> Result<ScriptBuf> {
        let script = self.multisig_script(user_pk, server_pk);
        tracing::debug!("Created checksigverify script for user: {}, server: {}", user_pk, server_pk);
        Ok(script)
    }

    // checksigadd script (threshold support)
    // pattern: A checksig B checksigadd 2 numequal
    pub fn create_checksigadd_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> Result<ScriptBuf> {
        let script = self.multi_a_script(2, user_pk, server_pk);
        tracing::debug!("Created checksigadd script for user: {}, server: {}", user_pk, server_pk);
        Ok(script)
    }

    // taproot script with forfeit and redeem leaves
    pub fn build_funding_transaction_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
        timelock: Sequence,
    ) -> Result<ScriptBuf> {
        // Forfeit script: User checksigverify Server checksig
        let forfeit_script = self.multisig_script(user_pk, server_pk);
        
        // Redeem script: timelock CSV drop User checksigverify Server checksig
        let redeem_script = self.csv_sig_script(timelock, user_pk, server_pk);

        // Use unspendable key for taproot
        let unspendable_key: PublicKey = UNSPENDABLE_KEY.parse()
            .map_err(|e| anyhow!("Invalid unspendable key: {}", e))?;
        let (unspendable_key, _) = unspendable_key.inner.x_only_public_key();
        
        let secp = Secp256k1::new();
        let script_tree = TaprootBuilder::new()
            .add_leaf(1, forfeit_script)
            .map_err(|e| anyhow!("Failed to add forfeit leaf: {}", e))?
            .add_leaf(1, redeem_script)
            .map_err(|e| anyhow!("Failed to add redeem leaf: {}", e))?
            .finalize(&secp, unspendable_key)
            .map_err(|e| anyhow!("Failed to finalize script tree: {:?}", e))?;
        
        let output_key = script_tree.output_key();
        
        let script = Builder::new()
            .push_opcode(opcodes::all::OP_PUSHNUM_1)
            .push_slice(output_key.serialize())
            .into_script();

        tracing::debug!("Built funding transaction script with taproot output key");
        Ok(script)
    }

    // VTXO script (forfeit + exit paths)
    pub fn create_vtxo_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
        exit_delay: u32,
    ) -> Result<(ScriptBuf, ScriptBuf)> {
        // Forfeit path: user CHECKSIGVERIFY server CHECKSIG
        let forfeit_script = ScriptBuf::builder()
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script();

        // Exit path: delay CSV DROP user CHECKSIG (not CHECKSIGVERIFY)
        let exit_script = ScriptBuf::builder()
            .push_int(exit_delay as i64)
            .push_opcode(opcodes::all::OP_CSV)
            .push_opcode(opcodes::all::OP_DROP)
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script();

        Ok((forfeit_script, exit_script))
    }

    // shared output script for VTXO tree
    pub fn create_shared_output_script(
        &self,
        participants: &[XOnlyPublicKey],
        server_pk: XOnlyPublicKey,
        sweep_delay: u32,
    ) -> Result<(ScriptBuf, ScriptBuf)> {
        // Unroll path: n-of-n multisig of all participants
        let mut unroll_script = ScriptBuf::builder();
        
        // add all participant keys with CHECKSIGVERIFY (except last)
        for (i, pk) in participants.iter().enumerate() {
            unroll_script = unroll_script.push_x_only_key(pk);
            if i < participants.len() - 1 {
                unroll_script = unroll_script.push_opcode(opcodes::all::OP_CHECKSIGVERIFY);
            } else {
                unroll_script = unroll_script.push_opcode(opcodes::all::OP_CHECKSIG);
            }
        }
        
        let unroll_script = unroll_script.into_script();

        // Sweep path: server after timeout
        let sweep_script = ScriptBuf::builder()
            .push_int(sweep_delay as i64)
            .push_opcode(opcodes::all::OP_CSV)
            .push_opcode(opcodes::all::OP_DROP)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script();

        Ok((unroll_script, sweep_script))
    }

    pub fn validate_script_params(
        &self,
        user_pk: &XOnlyPublicKey,
        server_pk: &XOnlyPublicKey,
        delay: u32,
    ) -> Result<()> {
        if user_pk == server_pk {
            return Err(anyhow!("User and server public keys cannot be the same"));
        }

        // delay is reasonable (between 1 hour and 1 month)
        if delay < 3600 || delay > 2592000 {
            return Err(anyhow!("Exit delay must be between 1 hour and 30 days"));
        }

        Ok(())
    }

    // script type preference based on configuration
    pub fn get_preferred_script_type(&self) -> ScriptType {
        // for n-of-n multisig, checksigverify is preferred
        ScriptType::CheckSigVerify
    }
}

impl Clone for ScriptManager {
    fn clone(&self) -> Self {
        Self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScriptType {
    CheckSigVerify, // A checksigverify B checksigverify
    CheckSigAdd,    // A checksig B checksigadd N numequal
    MultiA,         // multi_a(threshold, pk1, pk2, ...)
}