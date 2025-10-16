use crate::*;
use anchor_lang::prelude::*;
use oapp::{
    endpoint::{
        cpi::accounts::Clear,
        instructions::ClearParams,
        ConstructCPIContext, ID as ENDPOINT_ID,
    },
    LzReceiveParams,
};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

#[derive(Accounts)]
#[instruction(params: LzReceiveParams)]
pub struct LzReceive<'info> {
    /// OApp Store PDA.  This account represents the "address" of your OApp on
    /// Solana and can contain any state relevant to your application.
    /// Customize the fields in `Store` as needed.
    #[account(mut, seeds = [STORE_SEED], bump = store.bump)]
    pub store: Account<'info, Store>,
    /// Peer config PDA for the sending chain. Ensures `params.sender` can only be the allowed peer from that remote chain.
    #[account(
        seeds = [PEER_SEED, &store.key().to_bytes(), &params.src_eid.to_be_bytes()],
        bump = peer.bump,
        constraint = params.sender == peer.peer_address
    )]
    pub peer: Account<'info, PeerConfig>,
    // Token context for executing payment on Solana
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = store,
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub merchant_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl LzReceive<'_> {
    pub fn apply(ctx: &mut Context<LzReceive>, params: &LzReceiveParams) -> Result<()> {
        // The OApp Store PDA is used to sign the CPI to the Endpoint program.
        let seeds: &[&[u8]] = &[STORE_SEED, &[ctx.accounts.store.bump]];

        // The first Clear::MIN_ACCOUNTS_LEN accounts were returned by
        // `lz_receive_types` and are required for Endpoint::clear
        let accounts_for_clear = &ctx.remaining_accounts[0..Clear::MIN_ACCOUNTS_LEN];//是 LayerZero 调用你的 lz_receive 时额外传入的账户，主要是为了 Endpoint::clear 能操作链上状态。
        //accounts_for_clear 本身 不是要被直接修改的账户，而是 Endpoint 程序用来找到它自己存储（state/PDA）去执行 clear 的“辅助账户”。
        // Call the Endpoint::clear CPI to clear the message from the Endpoint program.
        // This is necessary to ensure the message is processed only once and to
        // prevent replays.
        let _ = oapp::endpoint_cpi::clear(
            ENDPOINT_ID,
            ctx.accounts.store.key(),
            accounts_for_clear,
            seeds,
            ClearParams {
                receiver: ctx.accounts.store.key(),
                src_eid: params.src_eid,
                sender: params.sender,
                nonce: params.nonce,
                guid: params.guid,
                message: params.message.clone(),
            },
        )?;

        // Decode payment and transfer from vault to merchant
        let payment = msg_codec::decode_payment(&params.message)
            .map_err(|_| error!(MyOAppError::InvalidMessageType))?;

        // Validate that token.address is a Solana mint (32 bytes)
        require!(payment.token.addr.len() == 32, MyOAppError::InvalidMessageType);
        let token_mint = Pubkey::new_from_array(
            payment
                .token
                .addr
                .as_slice()
                .try_into()
                .map_err(|_| error!(MyOAppError::InvalidMessageType))?,
        );
        require_keys_eq!(ctx.accounts.mint.key(), token_mint);//这俩都是endpoint传过来的

        // Optionally, ensure recipient is Solana 32-byte owner
        require!(payment.recipient.addr.len() == 32, MyOAppError::InvalidMessageType);

        // Transfer using store PDA as signer
        let seeds: &[&[u8]] = &[STORE_SEED, &[ctx.accounts.store.bump]];
        let signer = &[seeds];
        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_ata.to_account_info(),
            to: ctx.accounts.merchant_ata.to_account_info(),
            authority: ctx.accounts.store.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, payment.amount)?;

        Ok(())
    }
}

