use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Mint, Token, TokenAccount},
    associated_token::AssociatedToken,
};

declare_id!("2unYtsTQXE8zSsFhYEZe77DLUDX4ba53vhzjAtNGUjhN");

#[error_code]
pub enum ErrorCode {
    #[msg("Game already started for this player.")]
    GameAlreadyStarted,
    #[msg("Game not started.")]
    GameNotStarted,
    #[msg("Game session expired.")]
    GameExpired,
    #[msg("Invalid game key.")]
    InvalidGameKey,
    #[msg("Score already logged.")]
    ScoreAlreadyLogged,
    #[msg("Name too long.")]
    NameTooLong,
    #[msg("Invalid prize distribution.")]
    InvalidPrizeDistribution,
    #[msg("Invalid entry fee.")]
    InvalidEntryFee,
    #[msg("Unauthorized. Only owner can perform this action.")]
    Unauthorized,
    #[msg("Player not found in leaderboard.")]
    PlayerNotInLeaderboard,
    #[msg("Not eligible for prize.")]
    NotEligibleForPrize,
}

#[program]
pub mod bonk_arena {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        entry_fee: u64,
        prize_ratio: u8,
        prize_distribution: [u8; 3],
    ) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        
        // Verify prize ratio is less than 100 and commission ratio is the remainder
        if prize_ratio >= 100 {
            return Err(ErrorCode::InvalidEntryFee.into());
        }
        
        // 验证奖金分配比例
        let total: u8 = prize_distribution.iter().sum();
        if total != 100 {
            return Err(ErrorCode::InvalidPrizeDistribution.into());
        }

        // 基本参数设置
        leaderboard.entry_fee = entry_fee;
        leaderboard.prize_ratio = prize_ratio;
        leaderboard.commission_ratio = 100 - prize_ratio;
        leaderboard.prize_distribution = prize_distribution;

        // BONK 相关设置
        leaderboard.token_mint = ctx.accounts.token_mint.key();
        leaderboard.owner_token_account = ctx.accounts.owner_token_account.key();

        // 初始化其他字段
        leaderboard.players = Vec::new();
        leaderboard.prize_pool = 0;
        leaderboard.commission_pool = 0;
        leaderboard.bump = ctx.bumps.leaderboard;
        leaderboard.authority = ctx.accounts.payer.key();
        
        Ok(())
    }

    pub fn set_token_pool(ctx: Context<SetTokenPool>) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        leaderboard.token_pool = ctx.accounts.token_pool.key();
        Ok(())
    }

    pub fn start_game(ctx: Context<StartGame>, name: String) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        let game_session = &mut ctx.accounts.game_session;
        let payer = &ctx.accounts.payer;

        // 检查名称长度
        if name.chars().count() > 10 {
            return Err(ErrorCode::NameTooLong.into());
        }

        // 处理入场费
        let entry_fee = leaderboard.entry_fee;
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.payer_token_account.to_account_info(),
                    to: ctx.accounts.token_pool.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            ),
            entry_fee,
        )?;

        let prize_addition = entry_fee * leaderboard.prize_ratio as u64 / 100;
        let commission_addition = entry_fee * leaderboard.commission_ratio as u64 / 100;

        leaderboard.prize_pool += prize_addition;
        leaderboard.commission_pool += commission_addition;

        let clock = Clock::get()?;
        game_session.player_address = payer.key();
        game_session.name = name;
        game_session.start_time = clock.unix_timestamp;
        game_session.game_completed = false;
        game_session.bump = ctx.bumps.game_session;

        Ok(())
    }

    pub fn end_game(
        ctx: Context<EndGame>,
        score: u32,
        // submitted_game_key: [u8; 32],
    ) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        let game_session = &mut ctx.accounts.game_session;
        let clock = Clock::get()?;

        // 检查游戏是否过期（10分钟）
        if clock.unix_timestamp - game_session.start_time > 600 {
            game_session.game_completed = true;
            return Err(ErrorCode::GameExpired.into());
        }

        // 计算预期的游戏密钥
        // let expected_game_key = solana_program::keccak::hashv(&[
        //     game_session.player_address.as_ref(),
        //     game_session.start_time.to_le_bytes().as_ref()
        // ]);

        // 验证游戏是否已完成
        if game_session.game_completed {
            return Err(ErrorCode::ScoreAlreadyLogged.into());
        }
        // 验证游戏密钥
        // if expected_game_key.to_bytes() != submitted_game_key {
            // return Err(ErrorCode::InvalidGameKey.into());
        // }


        // 登录分数
        leaderboard.players.push(Player {
            address: game_session.player_address,
            score,
            name: format!("Player: {}", game_session.name),
            claimed: false,
        });
        leaderboard.players.sort_by(|a, b| b.score.cmp(&a.score));
        if leaderboard.players.len() > 10 {
            leaderboard.players.pop();
        }

        // 标记游戏完成
        game_session.game_completed = true;

        Ok(())
    }

    pub fn claim_prize(ctx: Context<ClaimPrize>) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        let player_address = ctx.accounts.player.key();
        
        // 检查玩家是否在排行榜上
        let player_rank = leaderboard.players.iter()
            .position(|p| p.address == player_address)
            .ok_or(ErrorCode::PlayerNotInLeaderboard)?;
            
        // 只有前三名可以领奖
        if player_rank >= 3 {
            return Err(ErrorCode::NotEligibleForPrize.into());
        }
        
        // 计算奖金金额
        let prize_amount = leaderboard.prize_pool * 
            leaderboard.prize_distribution[player_rank] as u64 / 100;
            
        // 转移奖金到玩家账户
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_pool.to_account_info(),
                    to: ctx.accounts.player_token_account.to_account_info(),
                    authority: leaderboard.to_account_info(),
                },
                &[&[
                    b"leaderboard",
                    &[leaderboard.bump],
                ]],
            ),
            prize_amount,
        )?;

        // 标记该玩家已领奖
        leaderboard.players[player_rank].claimed = true;
        
        Ok(())
    }

    pub fn add_prize_pool(ctx: Context<AddPrizePool>, amount: u64) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        
        // 转移代币到奖金池
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.contributor_token_account.to_account_info(),
                    to: ctx.accounts.token_pool.to_account_info(),
                    authority: ctx.accounts.contributor.to_account_info(),
                },
            ),
            amount,
        )?;

        // 更新奖金池金额
        leaderboard.prize_pool += amount;
        
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct Player {
    pub address: Pubkey,
    pub score: u32,
    #[max_len(10)]
    pub name: String,
    pub claimed: bool,
}

#[account]
#[derive(InitSpace)]
pub struct Leaderboard {
    pub entry_fee: u64,              // 参赛费
    pub prize_ratio: u8,             // 奖金池比例
    pub commission_ratio: u8,        // 抽成池比例
    pub prize_pool: u64,             // 奖金池
    pub commission_pool: u64,        // 抽成池
    pub prize_distribution: [u8; 3], // 前三名分配比例
    #[max_len(10)]
    pub players: Vec<Player>,        // 排行榜
    pub token_mint: Pubkey,          // 游戏代币的 mint 地址
    pub token_pool: Pubkey,          // 游戏代币池地址
    pub owner_token_account: Pubkey, // 合约拥有者的代币账户
    pub authority: Pubkey,           // 添加 authority 字段
    pub bump: u8,                    // PDA bump
}

#[account]
pub struct GameSession {
    pub player_address: Pubkey,    // 玩家地址
    pub name: String,              // 玩家名称
    pub start_time: i64,           // 开始时间
    pub game_completed: bool,      // 游戏是否完成
    pub bump: u8,                  // PDA bump
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + Leaderboard::INIT_SPACE,
        seeds = [b"leaderboard"],
        bump
    )]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub token_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub owner_token_account: Account<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct StartGame<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + 32 + 50 + 8 + 32 + 1 + 1,
        seeds = [b"player_session", payer.key().as_ref()],
        bump
    )]
    pub game_session: Account<'info, GameSession>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = payer
    )]
    pub payer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseRank<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub owner_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EndGame<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        mut,
        seeds = [b"player_session", payer.key().as_ref()],
        bump = game_session.bump,
        constraint = game_session.player_address == payer.key(),
        constraint = !game_session.game_completed @ ErrorCode::GameAlreadyStarted,
    )]
    pub game_session: Account<'info, GameSession>,
    #[account(mut)]
    pub payer: Signer<'info>,
}

#[derive(Accounts)]
pub struct AddPrizePool<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = contributor
    )]
    pub contributor_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub contributor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = leaderboard.token_mint,
        token::authority = player
    )]
    pub player_token_account: Account<'info, TokenAccount>,
    pub player: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SetTokenPool<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = token_mint,
        associated_token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    pub token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
