use crate::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

#[derive(Accounts)]
#[instruction(params: WithdrawLiquidityParams)]
pub struct WithdrawLiquidity<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [STORE_SEED], bump = store.bump, has_one = admin)]
    pub store: Account<'info, Store>,
    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = store,
    )]
    pub vault_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawLiquidity<'info> {
    pub fn apply(ctx: &mut Context<WithdrawLiquidity>, params: &WithdrawLiquidityParams) -> Result<()> {
        let seeds: &[&[u8]] = &[STORE_SEED, &[ctx.accounts.store.bump]];
        let signer = &[seeds];
        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_ata.to_account_info(),
            to: ctx.accounts.admin_ata.to_account_info(),
            authority: ctx.accounts.store.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, params.amount)?;
        Ok(())
    }
}

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawLiquidityParams {
    pub amount: u64,
}





