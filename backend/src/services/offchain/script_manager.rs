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

    /// Create CSV signature script (exit path with timelock)
    /// Based on your mentor's csv_sig_script pattern
    pub fn csv_sig_script(
        &self,
        locktime: Sequence,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> ScriptBuf {
        ScriptBuf::builder()
            .push_int(locktime.to_consensus_u32() as i64)
            .push_opcode(opcodes::all::OP_CSV)  // Using OP_CSV instead of OP_CHECKSEQUENCEVERIFY
            .push_opcode(opcodes::all::OP_DROP)
            .push_x_only_key(&user_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIG)
            .into_script()
    }

    /// Create multisig script (collaborative path)
    /// Based on your mentor's multisig_script pattern
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

    /// Create 3-party multisig script (user + counterparty + server)
    /// Based on your mentor's pattern: Alice checksigverify Bob checksigverify Server checksig
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

    /// Create multi_a script (checksigadd variant from arkade-os tapscripts)
    /// Based on the tapscripts vtxo example: multi_a(2, pk1, pk2)
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

    /// Create simple exit script (user-only with CSV delay)
    /// Based on arkade-os tapscripts: and_v(v:pk(user), older(delay))
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

    /// Create checksigverify script (preferred for n-of-n)
    /// A checksigverify B checksigverify (without final checksig)
    pub fn create_checksigverify_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> Result<ScriptBuf> {
        let script = self.multisig_script(user_pk, server_pk);
        tracing::debug!("Created checksigverify script for user: {}, server: {}", user_pk, server_pk);
        Ok(script)
    }

    /// Create checksigadd script (threshold support)
    /// A checksig B checksigadd 2 numequal
    pub fn create_checksigadd_script(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
    ) -> Result<ScriptBuf> {
        let script = self.multi_a_script(2, user_pk, server_pk);
        tracing::debug!("Created checksigadd script for user: {}, server: {}", user_pk, server_pk);
        Ok(script)
    }

    /// Build complete funding transaction script (based on your mentor's pattern)
    /// Creates a taproot script with forfeit and redeem leaves
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

    /// Create VTXO script based on arkade-os tapscripts pattern
    /// Uses multi_a for collaborative path and simple exit for unilateral path
    pub fn create_vtxo_script_arkade_style(
        &self,
        user_pk: XOnlyPublicKey,
        server_pk: XOnlyPublicKey,
        exit_delay: u32,
    ) -> Result<(ScriptBuf, ScriptBuf)> {
        // Collaborative path: multi_a(2, user, server)
        let collaborative_script = self.multi_a_script(2, user_pk, server_pk);
        
        // Exit path: and_v(v:pk(user), older(delay))
        let exit_script = self.create_exit_script(user_pk, exit_delay)?;

        tracing::debug!("Created arkade-style VTXO script set");
        Ok((collaborative_script, exit_script))
    }

    /// Create shared output script for unrolling
    /// Based on arkade-os tapscripts unroll pattern
    pub fn create_shared_output_script(
        &self,
        server_pk: XOnlyPublicKey,
        sweep_delay: u32,
    ) -> Result<ScriptBuf> {
        // This would use Elements introspection opcodes in a real implementation
        // For now, create a simple sweep script
        let sweep_script = ScriptBuf::builder()
            .push_x_only_key(&server_pk)
            .push_opcode(opcodes::all::OP_CHECKSIGVERIFY)
            .push_int(sweep_delay as i64)
            .push_opcode(opcodes::all::OP_CSV)
            .into_script();

        tracing::debug!("Created shared output script with sweep delay: {}", sweep_delay);
        Ok(sweep_script)
    }

    /// Validate script parameters
    pub fn validate_script_params(
        &self,
        user_pk: &XOnlyPublicKey,
        server_pk: &XOnlyPublicKey,
        delay: u32,
    ) -> Result<()> {
        // Validate public keys are not the same
        if user_pk == server_pk {
            return Err(anyhow!("User and server public keys cannot be the same"));
        }

        // Validate delay is reasonable (between 1 hour and 1 month)
        if delay < 3600 || delay > 2592000 {
            return Err(anyhow!("Exit delay must be between 1 hour and 30 days"));
        }

        Ok(())
    }

    /// Get script type preference based on configuration
    pub fn get_preferred_script_type(&self) -> ScriptType {
        // For n-of-n multisig, checksigverify is preferred
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