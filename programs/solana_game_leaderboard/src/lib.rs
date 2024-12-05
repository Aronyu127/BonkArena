use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("CimnUFrTgq9DoGx9peefTU2bRvkQSMLwra8ZgexZ1WQG");

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
    }

#[program]
pub mod solana_game_leaderboard {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        entry_fee: u64,
        prize_ratio: u8,
        commission_ratio: u8,
        prize_distribution: [u8; 3],
    ) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        
        // 验证费率总和
        if prize_ratio + commission_ratio != 100 {
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
        leaderboard.commission_ratio = commission_ratio;
        leaderboard.prize_distribution = prize_distribution;

        // BONK 相关设置
        leaderboard.token_mint = ctx.accounts.token_mint.key();
        leaderboard.token_pool = ctx.accounts.token_pool.key();
        leaderboard.owner_token_account = ctx.accounts.owner_token_account.key();

        // 初始化其他字段
        leaderboard.players = Vec::new();
        leaderboard.prize_pool = 0;
        leaderboard.commission_pool = 0;
        
        Ok(())
    }

    pub fn set_secret_key(ctx: Context<SetSecretKey>, new_secret_key: [u8; 32]) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        leaderboard.secret_key = new_secret_key;
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

        // 生成游戏密钥（只存储在合约中）
        let clock = Clock::get()?;
        let game_key = solana_program::keccak::hashv(&[
            payer.key().as_ref(),
            clock.unix_timestamp.to_le_bytes().as_ref(),
            leaderboard.secret_key.as_ref(),
        ]);

        // 初始化游戏会话
        game_session.player_address = payer.key();
        game_session.name = name;
        game_session.start_time = clock.unix_timestamp;
        game_session.game_key = game_key.to_bytes();
        game_session.game_completed = false;
        game_session.bump = ctx.bumps.game_session;

        Ok(())
    }

    pub fn log_score(
        ctx: Context<LogScore>,
        score: u32,
        submitted_game_key: [u8; 32],
    ) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        let game_session = &mut ctx.accounts.game_session;
        let clock = Clock::get()?;

        // 检查游戏是否过期（10分钟）
        if clock.unix_timestamp - game_session.start_time > 600 {
            game_session.game_completed = true;
            return Err(ErrorCode::GameExpired.into());
        }

        // 验证游戏密钥
        if game_session.game_key != submitted_game_key {
            game_session.game_completed = true;
            return Err(ErrorCode::InvalidGameKey.into());
        }

        // 验证游戏是否已完成
        if game_session.game_completed {
            return Err(ErrorCode::ScoreAlreadyLogged.into());
        }

        // 登录分数
        leaderboard.players.push(Player {
            address: game_session.player_address,
            score,
            name: format!("Player: {}", game_session.name),
        });
        leaderboard.players.sort_by(|a, b| b.score.cmp(&a.score));
        if leaderboard.players.len() > 10 {
            leaderboard.players.pop();
        }

        // 标记游戏完成
        game_session.game_completed = true;

        Ok(())
    }

    pub fn end_game(ctx: Context<EndGame>) -> Result<()> {
        let leaderboard = &mut ctx.accounts.leaderboard;
        // 奖金分配逻辑
        distribute_prizes(leaderboard)?;

        // 清空排行榜
        leaderboard.players.clear();
        leaderboard.prize_pool = 0;
        Ok(())
    }
}

// 工具函数：更新排行榜
fn update_leaderboard(leaderboard: &mut Account<Leaderboard>, player: Player) -> Result<()> {
    leaderboard.players.push(player);
    leaderboard.players.sort_by(|a, b| b.score.cmp(&a.score)); // 由高到低排序
    if leaderboard.players.len() > 10 {
        leaderboard.players.pop(); // 移除最后一名
    }
    Ok(())
}

// 工具函数：分配奖金
fn distribute_prizes(leaderboard: &mut Account<Leaderboard>) -> Result<()> {
    let total_prize = leaderboard.prize_pool;
    let prize_distribution = leaderboard.prize_distribution;
    
    // 计算实际获奖玩家数量（最多3名）
    let winner_count = leaderboard.players.len().min(3);
    
    // 分配奖金给实际获奖玩家
    for i in 0..winner_count {
        let prize = total_prize * prize_distribution[i] as u64 / 100;
        // 奖金转账逻辑给玩家
    }
    
    // 如果获奖玩家少于3名，剩余奖金转给合约拥有者
    if winner_count < 3 {
        let mut remaining_prize = 0;
        for i in winner_count..3 {
            remaining_prize += total_prize * prize_distribution[i] as u64 / 100;
        }
        // 将剩余奖金转给合约拥有者
    }
    
    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Player {
    pub address: Pubkey,
    pub score: u32,
    pub name: String,
}

#[account]
pub struct Leaderboard {
    pub entry_fee: u64,              // 参赛费
    pub prize_ratio: u8,             // 奖金池比例
    pub commission_ratio: u8,        // 抽成池比例
    pub prize_pool: u64,             // 奖金池
    pub commission_pool: u64,        // 抽成池
    pub prize_distribution: [u8; 3], // 前三名分配比例
    pub players: Vec<Player>,        // 排行榜
    pub secret_key: [u8; 32],        // 合约拥有者的 secret key
    pub token_mint: Pubkey,          // 游戏代币的 mint 地址
    pub token_pool: Pubkey,          // 游戏代币池地址
    pub owner_token_account: Pubkey, // 合约拥有者的代币账户
}

#[account]
pub struct GameSession {
    pub player_address: Pubkey,    // 玩家地址
    pub name: String,              // 玩家名称
    pub start_time: i64,           // 开始时间
    pub game_key: [u8; 32],        // 游戏密钥
    pub game_completed: bool,      // 游戏是否完成
    pub bump: u8,                  // PDA bump
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 8 + 1 + 1 + 8 + 8 + 3 + (4 + 10 * (32 + 8 + 50)) + 32 + 32 + 32 + 32
    )]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub token_mint: Account<'info, Mint>,
    /// CHECK: 这是一个代币池账户，会在初始化时创建
    #[account(mut)]
    pub token_pool: UncheckedAccount<'info>,
    /// CHECK: 这是合约拥有者的代币账户
    pub owner_token_account: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct StartGame<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(
        init,
        payer = payer,
        space = 8 + 32 + 50 + 8 + 32 + 1 + 1,
        seeds = [b"player_session", payer.key().as_ref()],
        bump
    )]
    pub game_session: Account<'info, GameSession>,
    #[account(
        mut,
        token::mint = token_mint,
        token::authority = payer
    )]
    pub payer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = token_mint,
        token::authority = leaderboard
    )]
    pub token_pool: Account<'info, TokenAccount>,
    pub token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EndGame<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
    #[account(mut)]
    pub token_pool: Account<'info, TokenAccount>,
    #[account(mut)]
    pub owner_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct LogScore<'info> {
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
pub struct SetSecretKey<'info> {
    #[account(mut)]
    pub leaderboard: Account<'info, Leaderboard>,
}
