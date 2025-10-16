use crate::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use oapp::endpoint::{instructions::SendParams, state::EndpointSettings, ENDPOINT_SEED, ID as ENDPOINT_ID};

//context结构体 它本质上是 Instruction 所需账户的集合。
#[derive(Accounts)]
#[instruction(params: SendPaymentParams)]
pub struct SendPayment<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,// 用户钱包，必须签名
    #[account(mut, seeds = [STORE_SEED], bump = store.bump)]
    /// OApp Store PDA that will also hold the vault ATA
    pub store: Account<'info, Store>,// PDA，持有 vault ATA
    pub mint: Account<'info, Mint>,// SPL Token mint 代币的mint账户
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = payer,
    )]
    pub payer_ata: Account<'info, TokenAccount>,//用户 ATA
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = store,
    )]
    pub vault_ata: Account<'info, TokenAccount>,//PDA ATA，存储资金
    #[account(
        seeds = [
            PEER_SEED,
            &store.key().to_bytes(),
            &params.dst_eid.to_be_bytes()
        ],
        bump = peer.bump
    )]
    /// Destination chain peer config used to get enforced options and receiver
    pub peer: Account<'info, PeerConfig>,// 跨链目标信息
    #[account(seeds = [ENDPOINT_SEED], bump = endpoint.bump, seeds::program = ENDPOINT_ID)]
    pub endpoint: Account<'info, EndpointSettings>,// Endpoint 配置
    pub token_program: Program<'info, Token>,//程序账户，用来Token转账
    pub associated_token_program: Program<'info, AssociatedToken>,//Anchor 会调用 associated_token_program 去创建 PDA 的 ATA，如果还没创建
    pub system_program: Program<'info, System>,//Solana 的 System Program，最基础的
}

impl<'info> SendPayment<'info> {
    pub fn apply(ctx: &mut Context<SendPayment>, params: &SendPaymentParams) -> Result<()> {
        // 1) Pull funds into vault (payer -> vault_ata)
        if params.amount > 0 {
            //cpi 需要的账户
            let cpi_accounts = Transfer {
                from: ctx.accounts.payer_ata.to_account_info(),
                to: ctx.accounts.vault_ata.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            };
            //构建cpi上下文 CpiContext 就是 跨程序调用的上下文，Anchor 封装了底层 invoke 调用
            let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
            //这才是执行 token 转账 需要上下文和参数
            token::transfer(cpi_ctx, params.amount)?;
        }

        // 2) Encode cross-chain payment message (Borsh via AnchorSerialize)
        let payment = msg_codec::PaymentMsg {
            sender: params.sender.clone(),
            recipient: params.recipient.clone(),
            token: params.token.clone(),
            amount: params.amount*0.97,
        };
        let message = msg_codec::encode_payment(&payment);

        // 3) Build send params and CPI to Endpoint
        let seeds: &[&[u8]] = &[STORE_SEED, &[ctx.accounts.store.bump]];
        let send_params = SendParams {
            dst_eid: params.dst_eid,//目标链 ID（哪个链收消息）
            receiver: ctx.accounts.peer.peer_address,//目标合约地址（目标链的 peer 地址）
            message,
            options: ctx
                .accounts
                .peer
                .enforced_options
                .combine_options(&None::<Vec<u8>>, &params.options)?,
            native_fee: params.native_fee,//目标链本地跨链手续费
            lz_token_fee: params.lz_token_fee,//LayerZero 跨链 token 费用
        };
        oapp::endpoint_cpi::send(
            ENDPOINT_ID,
            ctx.accounts.store.key(),//sender PDA
            ctx.remaining_accounts,
            seeds,//PDA 签名 seeds（程序代表 PDA 签名）
            send_params,//消息参数
        )?;

        Ok(())
    }
}

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct SendPaymentParams {
    pub dst_eid: u32,
    pub sender: msg_codec::AddressType,
    pub recipient: msg_codec::AddressType,
    pub token: msg_codec::AddressType,
    pub amount: u64,
    pub options: Vec<u8>,
    pub native_fee: u64,
    pub lz_token_fee: u64,
}


