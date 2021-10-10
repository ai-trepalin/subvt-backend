use crate::{
    crypto::AccountId,
    substrate::{
        argument::{get_argument_primitive, get_argument_vector, Argument, ArgumentPrimitive},
        error::DecodeError,
        metadata::Metadata,
        Block, MultiAddress,
    },
};
use log::{debug, warn};
use parity_scale_codec::{Compact, Decode, Input};
use crate::substrate::Chain;

#[derive(Debug)]
pub enum Timestamp {
    Set {
        version: u8,
        signature: Option<Signature>,
        timestamp: u64,
    },
}

impl Timestamp {
    pub fn from(
        name: &str,
        version: u8,
        signature: Option<Signature>,
        arguments: Vec<Argument>,
    ) -> Result<Option<SubstrateExtrinsic>, DecodeError> {
        let maybe_event = match name {
            "set" => Some(SubstrateExtrinsic::Timestamp(Timestamp::Set {
                version,
                signature,
                timestamp: get_argument_primitive!(&arguments[0], Moment).0,
            })),
            _ => None,
        };
        Ok(maybe_event)
    }
}

#[derive(Debug)]
pub enum Staking {
    Nominate {
        version: u8,
        signature: Option<Signature>,
        targets: Vec<MultiAddress>,
    },
}

impl Staking {
    pub fn from(
        name: &str,
        version: u8,
        signature: Option<Signature>,
        arguments: Vec<Argument>,
    ) -> Result<Option<SubstrateExtrinsic>, DecodeError> {
        let maybe_extrinsic = match name {
            "nominate" => Some(SubstrateExtrinsic::Staking(Staking::Nominate {
                version,
                signature,
                targets: get_argument_vector!(&arguments[0], MultiAddress),
            })),
            _ => None,
        };
        Ok(maybe_extrinsic)
    }
}

#[derive(Debug)]
pub enum SubstrateExtrinsic {
    Staking(Staking),
    Timestamp(Timestamp),
    Other {
        module_name: String,
        call_name: String,
        version: u8,
        signature: Option<Signature>,
    },
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub signer: MultiAddress,
    pub signature: sp_runtime::MultiSignature,
    pub era: Option<sp_runtime::generic::Era>,
    pub nonce: Option<u64>,
    pub tip: Option<u64>,
}

impl Signature {
    pub fn get_signer_account_id(&self) -> Option<AccountId> {
        self.signer.get_account_id()
    }
}

impl SubstrateExtrinsic {
    fn decode_extrinsic(chain: &Chain, metadata: &Metadata, bytes: &mut &[u8]) -> Result<Self, DecodeError> {
        let signed_version = bytes.read_byte().unwrap();
        let sign_mask = 0b10000000;
        let version_mask = 0b00000100;
        let is_signed = (signed_version & sign_mask) == sign_mask;
        let version = signed_version & version_mask;
        let signature = if is_signed {
            let signer = if metadata.is_signer_address_multi(chain) {
                MultiAddress::decode(&mut *bytes).unwrap()
            } else {
                MultiAddress::Id(Decode::decode(&mut *bytes).unwrap())
            };
            let signature = sp_runtime::MultiSignature::decode(&mut *bytes).unwrap();
            let era: sp_runtime::generic::Era = Decode::decode(&mut *bytes).unwrap();
            let nonce: Compact<u64> = Decode::decode(&mut *bytes).unwrap();
            let tip: Compact<u64> = Decode::decode(&mut *bytes).unwrap();
            let signature = Signature {
                signer,
                signature,
                era: Some(era),
                nonce: Some(nonce.0),
                tip: Some(tip.0),
            };
            Some(signature)
        } else {
            None
        };
        let module_index: u8 = Decode::decode(&mut *bytes).unwrap();
        let call_index: u8 = Decode::decode(&mut *bytes).unwrap();
        let module = metadata.modules.get(&module_index).unwrap();
        let call = module.calls.get(&call_index).unwrap();
        let maybe_extrinsic = match module.name.as_str() {
            "Timestamp" => {
                let mut arguments: Vec<Argument> = Vec::new();
                for argument_meta in &call.arguments {
                    arguments.push(Argument::decode(argument_meta, &mut *bytes).unwrap());
                }
                Timestamp::from(&call.name, version, signature.clone(), arguments.clone())?
            }
            "Staking" => {
                let mut arguments: Vec<Argument> = Vec::new();
                for argument_meta in &call.arguments {
                    let argument = Argument::decode(argument_meta, &mut *bytes).unwrap();
                    arguments.push(argument);
                }
                Staking::from(&call.name, version, signature.clone(), arguments.clone())?
            }
            _ => None,
        };
        let extrinsic = if let Some(extrinsic) = maybe_extrinsic {
            debug!("Decoded extrinsic {}.{}.", module.name, call.name);
            extrinsic
        } else {
            warn!("Non-specified extrinsic {}.{}.", module.name, call.name);
            SubstrateExtrinsic::Other {
                version,
                signature,
                module_name: module.name.clone(),
                call_name: call.name.clone(),
            }
        };
        Ok(extrinsic)
    }

    pub fn decode_extrinsics(chain: &Chain, metadata: &Metadata, block: Block) -> anyhow::Result<Vec<Self>> {
        let mut extrinsics: Vec<Self> = Vec::new();
        for extrinsic_hex_string in block.extrinsics {
            let mut raw_bytes: &[u8] = &hex::decode(extrinsic_hex_string.trim_start_matches("0x"))?;
            let byte_vector: Vec<u8> = Decode::decode(&mut raw_bytes).unwrap();
            let mut bytes: &[u8] = byte_vector.as_ref();
            extrinsics.push(SubstrateExtrinsic::decode_extrinsic(chain, metadata, &mut bytes)?);
        }
        Ok(extrinsics)
    }
}
