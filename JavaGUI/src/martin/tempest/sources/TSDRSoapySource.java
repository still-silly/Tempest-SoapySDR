/*******************************************************************************
 * Copyright (c) 2014 Martin Marinov.
 * All rights reserved. This program and the accompanying materials
 * are made available under the terms of the GNU Public License v3.0
 * which accompanies this distribution, and is available at
 * http://www.gnu.org/licenses/gpl.html
 *
 * Contributors:
 *     Martin Marinov - initial API and implementation
 ******************************************************************************/
package martin.tempest.sources;

import java.awt.Container;

/**
 * This plugin provides support for SoapySDR devices.
 *
 * @author Martin Marinov
 *
 */
public class TSDRSoapySource extends TSDRSource {

	public TSDRSoapySource() {
		super("SoapySDR device", "TSDRPlugin_Soapy", false);
	}

	@Override
	public boolean supportsSampleRateSelection() {
		return true;
	}

	@Override
	public boolean populateGUI(final Container cont, final String defaultprefs, final ActionListenerRegistrator okbutton) {
		final String prefs = (defaultprefs == null || defaultprefs.trim().isEmpty())
				? "driver=sdrplay" : defaultprefs;
		return super.populateGUI(cont, prefs, okbutton);
	}

}
