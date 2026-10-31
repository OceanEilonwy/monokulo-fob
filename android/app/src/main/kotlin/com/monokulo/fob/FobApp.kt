package com.monokulo.fob

import android.app.Application

/** Holds the one [WalletModel] the screens and [SyncService] share. */
class FobApp : Application() {
    val model by lazy { WalletModel(this) }
}
