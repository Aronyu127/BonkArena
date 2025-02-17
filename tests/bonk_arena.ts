import * as anchor from "@coral-xyz/anchor";
import { Program, BN, AnchorProvider } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import { assert } from "chai";

interface BonkArenaProgram extends Program<any> {
  account: {
    leaderboard: {
      fetch(address: PublicKey): Promise<any>;
    };
    gameSession: {
      fetch(address: PublicKey): Promise<any>;
    };
  };
}

describe("bonk_arena", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.BonkArena as BonkArenaProgram;
  const payer = Keypair.generate();
  
  // Test accounts
  let tokenMint: PublicKey;
  let ownerTokenAccount: PublicKey;
  let playerTokenAccount: PublicKey;
  let tokenPool: PublicKey;
  let leaderboard: PublicKey;
  let gameSession: PublicKey;
  
  // Constants
  const ENTRY_FEE = new BN(1_000_000_000); // 1 token
  const PRIZE_RATIO = 80;
  const PRIZE_DISTRIBUTION = [50, 30, 20];
  const INITIAL_SUPPLY = new BN(1_000_000_000_000); // 1000 tokens

  before(async () => {
    try {
      // Airdrop SOL to payer
      const signature = await provider.connection.requestAirdrop(
        payer.publicKey,
        2 * anchor.web3.LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(signature);

      // Create token mint
      tokenMint = await createMint(
        provider.connection,
        payer,
        payer.publicKey,
        null,
        9
      );

      // Create token accounts
      ownerTokenAccount = await createAssociatedTokenAccount(
        provider.connection,
        payer,
        tokenMint,
        payer.publicKey
      );

      playerTokenAccount = await createAssociatedTokenAccount(
        provider.connection,
        payer,
        tokenMint,
        payer.publicKey
      );

      // Mint tokens to player
      await mintTo(
        provider.connection,
        payer,
        tokenMint,
        playerTokenAccount,
        payer.publicKey,
        INITIAL_SUPPLY.toNumber()
      );

      // Get PDA for leaderboard
      [leaderboard] = PublicKey.findProgramAddressSync(
        [Buffer.from("leaderboard")],
        program.programId
      );

      // Get token pool address
      tokenPool = await getAssociatedTokenAddress(
        tokenMint,
        leaderboard,
        true
      );

      // Get game session PDA
      [gameSession] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("player_session"),
          payer.publicKey.toBuffer(),
        ],
        program.programId
      );

    } catch (error) {
      console.error("Setup error:", error);
      throw error;
    }
  });

  it("Initialize game", async () => {
    try {
      await program.methods
        .initialize(
          ENTRY_FEE,
          PRIZE_RATIO,
          PRIZE_DISTRIBUTION
        )
        .accounts({
          leaderboard,
          payer: payer.publicKey,
          tokenMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          ownerTokenAccount,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const account = await program.account.leaderboard.fetch(leaderboard);
      assert.equal(account.entryFee.toString(), ENTRY_FEE.toString());
      assert.equal(account.prizeRatio, PRIZE_RATIO);
      assert.equal(account.commissionRatio, 100 - PRIZE_RATIO);
      assert.deepEqual(account.prizeDistribution, PRIZE_DISTRIBUTION);
    } catch (error) {
      console.error("Initialize error:", error);
      throw error;
    }
  });

  it("Set token pool", async () => {
    try {
      await program.methods
        .setTokenPool()
        .accounts({
          leaderboard,
          tokenPool,
          tokenMint,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .rpc();

      const account = await program.account.leaderboard.fetch(leaderboard);
      assert.equal(account.tokenPool.toBase58(), tokenPool.toBase58());
    } catch (error) {
      console.error("Set token pool error:", error);
      throw error;
    }
  });

  it("Start game", async () => {
    try {
      const playerName = "Player1";
      
      await program.methods
        .startGame(playerName)
        .accounts({
          leaderboard,
          gameSession,
          payerTokenAccount: playerTokenAccount,
          tokenPool,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      const gameSessionAccount = await program.account.gameSession.fetch(gameSession);
      assert.equal(gameSessionAccount.name, playerName);
      assert.equal(
        gameSessionAccount.playerAddress.toBase58(),
        payer.publicKey.toBase58()
      );
      assert.equal(gameSessionAccount.gameCompleted, false);
    } catch (error) {
      console.error("Start game error:", error);
      throw error;
    }
  });

  it("End game", async () => {
    try {
      const score = 1000;
      
      await program.methods
        .endGame(score)
        .accounts({
          leaderboard,
          gameSession,
          payer: payer.publicKey,
        })
        .rpc();

      const gameSessionAccount = await program.account.gameSession.fetch(gameSession);
      assert.equal(gameSessionAccount.gameCompleted, true);

      const leaderboardAccount = await program.account.leaderboard.fetch(leaderboard);
      const playerEntry = leaderboardAccount.players.find(
        p => p.address.equals(payer.publicKey) && p.score === score
      );
      assert(playerEntry, "Player entry not found");
    } catch (error) {
      console.error("End game error:", error);
      throw error;
    }
  });

  it("Claim prize", async () => {
    try {
      const beforeBalance = await provider.connection.getTokenAccountBalance(
        playerTokenAccount
      );

      await program.methods
        .claimPrize()
        .accounts({
          leaderboard,
          tokenPool,
          playerTokenAccount,
          player: payer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      const afterBalance = await provider.connection.getTokenAccountBalance(
        playerTokenAccount
      );
      assert(
        Number(afterBalance.value.amount) > Number(beforeBalance.value.amount),
        "Prize not received"
      );

      const leaderboardAccount = await program.account.leaderboard.fetch(leaderboard);
      const playerEntry = leaderboardAccount.players.find(
        p => p.address.equals(payer.publicKey)
      );
      assert(playerEntry.claimed, "Prize not marked as claimed");
    } catch (error) {
      console.error("Claim prize error:", error);
      throw error;
    }
  });

  it("Add to prize pool", async () => {
    try {
      const amount = new BN(500_000_000);
      
      await program.methods
        .addPrizePool(amount)
        .accounts({
          leaderboard,
          contributorTokenAccount: playerTokenAccount,
          tokenPool,
          contributor: payer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      const leaderboardAccount = await program.account.leaderboard.fetch(leaderboard);
      assert(leaderboardAccount.prizePool.gt(new BN(0)), "Prize pool not increased");
    } catch (error) {
      console.error("Add prize pool error:", error);
      throw error;
    }
  });
});
